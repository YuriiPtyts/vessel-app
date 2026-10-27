// Fixture instrumentation for the repository's own native test process.
// Reject mpv's input channel-layout setup and audit its device listener lifetime.
#include <AudioToolbox/AudioToolbox.h>
#include <CoreAudio/CoreAudio.h>
#include <dlfcn.h>
#include <pthread.h>
#include <stdbool.h>
#include <string.h>

struct registration {
    AudioObjectID object;
    AudioObjectPropertyAddress property;
    AudioObjectPropertyListenerProc callback;
    void *context;
};
static struct registration records[64];
static pthread_mutex_t lock = PTHREAD_MUTEX_INITIALIZER;
static bool overflow;

static bool from_mpv(const void *address) {
    Dl_info image;
    return dladdr(address, &image) && image.dli_fname && strstr(image.dli_fname, "libmpv");
}
static bool matches(struct registration *record, AudioObjectID object,
                    const AudioObjectPropertyAddress *property,
                    AudioObjectPropertyListenerProc callback, void *context) {
    return record->callback == callback && record->context == context && record->object == object
        && record->property.mSelector == property->mSelector
        && record->property.mScope == property->mScope
        && record->property.mElement == property->mElement;
}
int vessel_audio_listener_count(void) {
    pthread_mutex_lock(&lock);
    int count = overflow ? 10000 : 0;
    for (int i = 0; i < 64; i++) if (records[i].callback) count++;
    pthread_mutex_unlock(&lock);
    return count;
}
static OSStatus audit_add(AudioObjectID object, const AudioObjectPropertyAddress *property,
                         AudioObjectPropertyListenerProc callback, void *context) {
    OSStatus status = AudioObjectAddPropertyListener(object, property, callback, context);
    if (status == noErr && from_mpv((const void *)callback)) {
        pthread_mutex_lock(&lock);
        int empty = -1;
        bool duplicate = false;
        for (int i = 0; i < 64; i++) {
            if (!records[i].callback) empty = i;
            if (matches(&records[i], object, property, callback, context)) duplicate = true;
        }
        if (!duplicate) {
            if (empty >= 0) records[empty] = (struct registration){ object, *property, callback, context };
            else overflow = true;
        }
        pthread_mutex_unlock(&lock);
    }
    return status;
}
static OSStatus audit_remove(AudioObjectID object, const AudioObjectPropertyAddress *property,
                            AudioObjectPropertyListenerProc callback, void *context) {
    OSStatus status = AudioObjectRemovePropertyListener(object, property, callback, context);
    if (status == noErr && from_mpv((const void *)callback)) {
        pthread_mutex_lock(&lock);
        for (int i = 0; i < 64; i++) {
            if (matches(&records[i], object, property, callback, context)) records[i].callback = NULL;
        }
        pthread_mutex_unlock(&lock);
    }
    return status;
}
static OSStatus fail_channel_layout(AudioUnit unit, AudioUnitPropertyID property,
                                   AudioUnitScope scope, AudioUnitElement element,
                                   const void *data, UInt32 size) {
    if (((property == kAudioUnitProperty_AudioChannelLayout && scope == kAudioUnitScope_Input)
         || (property == kAudioOutputUnitProperty_ChannelMap && scope == kAudioUnitScope_Global))
        && from_mpv(__builtin_return_address(0))) return kAudio_ParamError;
    return AudioUnitSetProperty(unit, property, scope, element, data, size);
}
#define INTERPOSE(replacement, original) \
    __attribute__((used)) static struct { const void *new_fn; const void *old_fn; } \
    pair_##original __attribute__((section("__DATA,__interpose"))) = \
        { (const void *)&replacement, (const void *)&original };
INTERPOSE(audit_add, AudioObjectAddPropertyListener)
INTERPOSE(audit_remove, AudioObjectRemovePropertyListener)
INTERPOSE(fail_channel_layout, AudioUnitSetProperty)
