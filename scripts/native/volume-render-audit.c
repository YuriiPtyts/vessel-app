// Fixture-only observer of actual mpv CoreAudio render samples. Samples are zeroed
// after observation, before they reach the output device; no test tone is audible.
#include <AudioToolbox/AudioToolbox.h>
#include <dlfcn.h>
#include <math.h>
#include <stdbool.h>
#include <stdatomic.h>
#include <stdint.h>
#include <string.h>

struct render_state {
    AudioUnit unit;
    AURenderCallbackStruct original;
    AudioStreamBasicDescription format;
    bool seen_frame;
};
static struct render_state states[8];
static _Atomic double latest_peak;
static _Atomic uint64_t blocks;
static _Atomic unsigned int block_frames;
static _Atomic uint64_t generations;
static _Atomic double first_gain;

extern double vessel_avfoundation_peak(void);
extern uint64_t vessel_avfoundation_blocks(void);
extern unsigned int vessel_avfoundation_frames(void);
extern uint64_t vessel_avfoundation_generations(void);
extern double vessel_avfoundation_first_gain(void);

uint64_t vessel_volume_output_generations(void) {
    return vessel_avfoundation_generations() + atomic_load(&generations);
}
double vessel_volume_output_first_gain(void) {
    return vessel_avfoundation_generations() ? vessel_avfoundation_first_gain() : atomic_load(&first_gain);
}

double vessel_volume_render_peak(void) {
    return vessel_avfoundation_blocks() ? vessel_avfoundation_peak() : atomic_load(&latest_peak);
}
uint64_t vessel_volume_render_blocks(void) {
    return vessel_avfoundation_blocks() + atomic_load(&blocks);
}
unsigned int vessel_volume_render_frames(void) {
    return vessel_avfoundation_blocks() ? vessel_avfoundation_frames() : atomic_load(&block_frames);
}

double vessel_volume_buffer_peak(const AudioBuffer *buffer, const AudioStreamBasicDescription *format) {
    double result = 0;
    if (format->mFormatFlags & kAudioFormatFlagIsFloat) {
        if (format->mBitsPerChannel == 32) {
            const float *samples = buffer->mData;
            for (UInt32 i = 0; i < buffer->mDataByteSize / sizeof(float); i++)
                result = fmax(result, fabs(samples[i]));
        } else if (format->mBitsPerChannel == 64) {
            const double *samples = buffer->mData;
            for (UInt32 i = 0; i < buffer->mDataByteSize / sizeof(double); i++)
                result = fmax(result, fabs(samples[i]));
        } else return -1;
    } else if (format->mFormatFlags & kAudioFormatFlagIsSignedInteger) {
        if (format->mBitsPerChannel == 16) {
            const int16_t *samples = buffer->mData;
            for (UInt32 i = 0; i < buffer->mDataByteSize / sizeof(int16_t); i++)
                result = fmax(result, fabs(samples[i] / 32768.0));
        } else if (format->mBitsPerChannel == 32) {
            const int32_t *samples = buffer->mData;
            for (UInt32 i = 0; i < buffer->mDataByteSize / sizeof(int32_t); i++)
                result = fmax(result, fabs(samples[i] / 2147483648.0));
        } else return -1;
    } else return -1;
    return result;
}

static OSStatus observe(void *context, AudioUnitRenderActionFlags *flags,
                        const AudioTimeStamp *time, UInt32 bus, UInt32 frames,
                        AudioBufferList *data) {
    struct render_state *state = context;
    OSStatus status = state->original.inputProc(state->original.inputProcRefCon,
                                               flags, time, bus, frames, data);
    double measured = 0;
    if (data) {
        for (UInt32 i = 0; i < data->mNumberBuffers; i++) {
            AudioBuffer *buffer = &data->mBuffers[i];
            if (!buffer->mData) continue;
            double value = vessel_volume_buffer_peak(buffer, &state->format);
            measured = value < 0 ? -1 : fmax(measured, value);
            memset(buffer->mData, 0, buffer->mDataByteSize);
        }
    }
    // HAL gain is applied by the AudioUnit after its input render callback.
    // Observe that actual per-unit parameter as part of the pre-silencing envelope.
    AudioUnitParameterValue gain = 1;
    if (AudioUnitGetParameter(state->unit, kHALOutputParam_Volume,
                             kAudioUnitScope_Global, 0, &gain) == noErr)
        measured *= gain;
    if (!state->seen_frame) {
        state->seen_frame = true;
        atomic_store(&first_gain, gain);
    }
    *flags |= kAudioUnitRenderAction_OutputIsSilence;
    atomic_store(&latest_peak, measured);
    atomic_store(&block_frames, frames);
    atomic_fetch_add(&blocks, 1);
    return status;
}

static OSStatus audit_property(AudioUnit unit, AudioUnitPropertyID property,
                               AudioUnitScope scope, AudioUnitElement element,
                               const void *data, UInt32 size) {
    if (property == kAudioUnitProperty_SetRenderCallback && data
        && size == sizeof(AURenderCallbackStruct)) {
        const AURenderCallbackStruct *callback = data;
        Dl_info image;
        if (callback->inputProc && dladdr((const void *)callback->inputProc, &image)
            && image.dli_fname && strstr(image.dli_fname, "libmpv")) {
            for (unsigned int i = 0; i < 8; i++) {
                if (states[i].unit && states[i].unit != unit) continue;
                states[i].unit = unit;
                states[i].original = *callback;
                states[i].seen_frame = false;
                atomic_fetch_add(&generations, 1);
                UInt32 bytes = sizeof(states[i].format);
                AudioUnitGetProperty(unit, kAudioUnitProperty_StreamFormat,
                                     kAudioUnitScope_Input, 0, &states[i].format, &bytes);
                AURenderCallbackStruct wrapped = { observe, &states[i] };
                return AudioUnitSetProperty(unit, property, scope, element,
                                            &wrapped, sizeof(wrapped));
            }
        }
    }
    OSStatus status = AudioUnitSetProperty(unit, property, scope, element, data, size);
    if (status == noErr && property == kAudioUnitProperty_StreamFormat
        && scope == kAudioUnitScope_Input && size == sizeof(AudioStreamBasicDescription)) {
        for (unsigned int i = 0; i < 8; i++)
            if (states[i].unit == unit) memcpy(&states[i].format, data, size);
    }
    return status;
}

__attribute__((used)) static struct { const void *new_fn; const void *old_fn; }
interpose __attribute__((section("__DATA,__interpose"))) = {
    (const void *)&audit_property, (const void *)&AudioUnitSetProperty
};
