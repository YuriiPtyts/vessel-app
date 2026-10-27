// Fixture observer of AVFoundation's real PCM queue and live render timeline.
// The renderer is muted before feeding audio. Reading its scheduled PCM envelope
// times its native volume gives the pre-silencing gain, not an acoustic recording.
#import <AVFoundation/AVFoundation.h>
#import <CoreMedia/CoreMedia.h>
#import <objc/runtime.h>
#include <pthread.h>
#include <stdatomic.h>

extern double vessel_volume_buffer_peak(const AudioBuffer *, const AudioStreamBasicDescription *);
static AVSampleBufferAudioRenderer *renderer;
static AVSampleBufferRenderSynchronizer *synchronizer;
static void (*original_add)(id, SEL, id);
static void (*original_enqueue)(id, SEL, CMSampleBufferRef);
static pthread_mutex_t queue_lock = PTHREAD_MUTEX_INITIALIZER;
struct envelope { double start, end, peak; };
static struct envelope envelopes[128];
static _Atomic uint64_t queued;
static _Atomic unsigned int latest_frames;
static _Atomic uint64_t generations;
static _Atomic double first_gain;
static bool seen_buffer;

uint64_t vessel_avfoundation_blocks(void) { return atomic_load(&queued); }
unsigned int vessel_avfoundation_frames(void) { return atomic_load(&latest_frames); }
uint64_t vessel_avfoundation_generations(void) { return atomic_load(&generations); }
double vessel_avfoundation_first_gain(void) { return atomic_load(&first_gain); }
double vessel_avfoundation_peak(void) {
    if (!synchronizer || !renderer) return 0;
    double now = CMTimeGetSeconds(synchronizer.currentTime);
    double peak = 0;
    pthread_mutex_lock(&queue_lock);
    for (unsigned int i = 0; i < 128; i++)
        if (envelopes[i].start <= now && now < envelopes[i].end) peak = envelopes[i].peak;
    pthread_mutex_unlock(&queue_lock);
    return peak * renderer.volume;
}

static void audit_add(id object, SEL selector, id output) {
    if ([output isKindOfClass:AVSampleBufferAudioRenderer.class]) {
        renderer = output;
        synchronizer = object;
        renderer.muted = YES;
        seen_buffer = false;
        atomic_fetch_add(&generations, 1);
    }
    original_add(object, selector, output);
}

static void audit_enqueue(id object, SEL selector, CMSampleBufferRef sample) {
    if (object == renderer) {
        // Keep the physical output silent independently of both app gain paths.
        renderer.muted = YES;
        if (!seen_buffer) {
            seen_buffer = true;
            atomic_store(&first_gain, renderer.volume);
        }
        CMBlockBufferRef block = CMSampleBufferGetDataBuffer(sample);
        const AudioStreamBasicDescription *format = CMAudioFormatDescriptionGetStreamBasicDescription(
            CMSampleBufferGetFormatDescription(sample));
        char *data = NULL;
        size_t length = 0;
        if (block && format && CMBlockBufferGetDataPointer(block, 0, NULL, &length, &data) == noErr) {
            AudioBuffer buffer = {format->mChannelsPerFrame, (UInt32)length, data};
            double peak = vessel_volume_buffer_peak(&buffer, format);
            double start = CMTimeGetSeconds(CMSampleBufferGetPresentationTimeStamp(sample));
            unsigned int frames = (unsigned int)CMSampleBufferGetNumSamples(sample);
            uint64_t index = atomic_load(&queued);
            pthread_mutex_lock(&queue_lock);
            envelopes[index % 128] = (struct envelope){start, start + frames / format->mSampleRate, peak};
            pthread_mutex_unlock(&queue_lock);
            atomic_store(&latest_frames, frames);
            atomic_fetch_add(&queued, 1);
        }
    }
    original_enqueue(object, selector, sample);
}

__attribute__((constructor)) static void install_observers(void) {
    Method add = class_getInstanceMethod(AVSampleBufferRenderSynchronizer.class, @selector(addRenderer:));
    Method enqueue = class_getInstanceMethod(AVSampleBufferAudioRenderer.class, @selector(enqueueSampleBuffer:));
    if (add && enqueue) {
        original_add = (void *)method_setImplementation(add, (IMP)audit_add);
        original_enqueue = (void *)method_setImplementation(enqueue, (IMP)audit_enqueue);
    }
}
