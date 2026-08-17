#include <stddef.h>
#include <stdio.h>
#include "nvEncodeAPI.h"

#define FIELD(type, field) printf(#type "." #field "=%zu\n", offsetof(type, field))
#define NESTED(type, outer, nested_type, inner) \
    printf(#type "." #outer "." #inner "=%zu\n", \
           offsetof(type, outer) + offsetof(nested_type, inner))

int main(void) {
    printf("ptr=%zu\n", sizeof(void *));
    printf("config.size=%zu align=%zu\n", sizeof(NV_ENC_CONFIG), _Alignof(NV_ENC_CONFIG));
    FIELD(NV_ENC_CONFIG, version);
    FIELD(NV_ENC_CONFIG, profileGUID);
    FIELD(NV_ENC_CONFIG, gopLength);
    FIELD(NV_ENC_CONFIG, frameIntervalP);
    FIELD(NV_ENC_CONFIG, frameFieldMode);
    FIELD(NV_ENC_CONFIG, mvPrecision);
    FIELD(NV_ENC_CONFIG, rcParams);
    FIELD(NV_ENC_CONFIG, encodeCodecConfig);
    NESTED(NV_ENC_CONFIG, rcParams, NV_ENC_RC_PARAMS, version);
    NESTED(NV_ENC_CONFIG, rcParams, NV_ENC_RC_PARAMS, rateControlMode);
    NESTED(NV_ENC_CONFIG, rcParams, NV_ENC_RC_PARAMS, averageBitRate);
    NESTED(NV_ENC_CONFIG, rcParams, NV_ENC_RC_PARAMS, maxBitRate);
    NESTED(NV_ENC_CONFIG, rcParams, NV_ENC_RC_PARAMS, vbvBufferSize);
    NESTED(NV_ENC_CONFIG, rcParams, NV_ENC_RC_PARAMS, vbvInitialDelay);
    NESTED(NV_ENC_CONFIG, rcParams, NV_ENC_RC_PARAMS, minQP);
    NESTED(NV_ENC_CONFIG, rcParams, NV_ENC_RC_PARAMS, targetQuality);
    NESTED(NV_ENC_CONFIG, rcParams, NV_ENC_RC_PARAMS, lookaheadDepth);
    NESTED(NV_ENC_CONFIG, rcParams, NV_ENC_RC_PARAMS, multiPass);
    NESTED(NV_ENC_CONFIG, encodeCodecConfig, NV_ENC_CONFIG_H264, idrPeriod);
    NESTED(NV_ENC_CONFIG, encodeCodecConfig, NV_ENC_CONFIG_H264, h264VUIParameters);
    printf("NV_ENC_CONFIG.encodeCodecConfig.h264VUIParameters.videoSignalTypePresentFlag=%zu\n",
           offsetof(NV_ENC_CONFIG, encodeCodecConfig) +
           offsetof(NV_ENC_CONFIG_H264, h264VUIParameters) +
           offsetof(NV_ENC_CONFIG_H264_VUI_PARAMETERS, videoSignalTypePresentFlag));
    printf("NV_ENC_CONFIG.encodeCodecConfig.h264VUIParameters.videoFormat=%zu\n",
           offsetof(NV_ENC_CONFIG, encodeCodecConfig) +
           offsetof(NV_ENC_CONFIG_H264, h264VUIParameters) +
           offsetof(NV_ENC_CONFIG_H264_VUI_PARAMETERS, videoFormat));
    printf("NV_ENC_CONFIG.encodeCodecConfig.h264VUIParameters.videoFullRangeFlag=%zu\n",
           offsetof(NV_ENC_CONFIG, encodeCodecConfig) +
           offsetof(NV_ENC_CONFIG_H264, h264VUIParameters) +
           offsetof(NV_ENC_CONFIG_H264_VUI_PARAMETERS, videoFullRangeFlag));
    printf("NV_ENC_CONFIG.encodeCodecConfig.h264VUIParameters.colourDescriptionPresentFlag=%zu\n",
           offsetof(NV_ENC_CONFIG, encodeCodecConfig) +
           offsetof(NV_ENC_CONFIG_H264, h264VUIParameters) +
           offsetof(NV_ENC_CONFIG_H264_VUI_PARAMETERS, colourDescriptionPresentFlag));
    printf("NV_ENC_CONFIG.encodeCodecConfig.h264VUIParameters.colourPrimaries=%zu\n",
           offsetof(NV_ENC_CONFIG, encodeCodecConfig) +
           offsetof(NV_ENC_CONFIG_H264, h264VUIParameters) +
           offsetof(NV_ENC_CONFIG_H264_VUI_PARAMETERS, colourPrimaries));
    printf("NV_ENC_CONFIG.encodeCodecConfig.h264VUIParameters.transferCharacteristics=%zu\n",
           offsetof(NV_ENC_CONFIG, encodeCodecConfig) +
           offsetof(NV_ENC_CONFIG_H264, h264VUIParameters) +
           offsetof(NV_ENC_CONFIG_H264_VUI_PARAMETERS, transferCharacteristics));
    printf("NV_ENC_CONFIG.encodeCodecConfig.h264VUIParameters.colourMatrix=%zu\n",
           offsetof(NV_ENC_CONFIG, encodeCodecConfig) +
           offsetof(NV_ENC_CONFIG_H264, h264VUIParameters) +
           offsetof(NV_ENC_CONFIG_H264_VUI_PARAMETERS, colourMatrix));
    NESTED(NV_ENC_CONFIG, encodeCodecConfig, NV_ENC_CONFIG_H264, chromaFormatIDC);
    NESTED(NV_ENC_CONFIG, encodeCodecConfig, NV_ENC_CONFIG_H264, outputBitDepth);
    NESTED(NV_ENC_CONFIG, encodeCodecConfig, NV_ENC_CONFIG_H264, inputBitDepth);

    printf("preset.size=%zu align=%zu\n", sizeof(NV_ENC_PRESET_CONFIG), _Alignof(NV_ENC_PRESET_CONFIG));
    FIELD(NV_ENC_PRESET_CONFIG, version);
    FIELD(NV_ENC_PRESET_CONFIG, presetCfg);

    printf("init.size=%zu align=%zu\n", sizeof(NV_ENC_INITIALIZE_PARAMS), _Alignof(NV_ENC_INITIALIZE_PARAMS));
    FIELD(NV_ENC_INITIALIZE_PARAMS, version);
    FIELD(NV_ENC_INITIALIZE_PARAMS, encodeGUID);
    FIELD(NV_ENC_INITIALIZE_PARAMS, presetGUID);
    FIELD(NV_ENC_INITIALIZE_PARAMS, encodeWidth);
    FIELD(NV_ENC_INITIALIZE_PARAMS, encodeHeight);
    FIELD(NV_ENC_INITIALIZE_PARAMS, darWidth);
    FIELD(NV_ENC_INITIALIZE_PARAMS, darHeight);
    FIELD(NV_ENC_INITIALIZE_PARAMS, frameRateNum);
    FIELD(NV_ENC_INITIALIZE_PARAMS, frameRateDen);
    FIELD(NV_ENC_INITIALIZE_PARAMS, enableEncodeAsync);
    FIELD(NV_ENC_INITIALIZE_PARAMS, enablePTD);
    FIELD(NV_ENC_INITIALIZE_PARAMS, privDataSize);
    FIELD(NV_ENC_INITIALIZE_PARAMS, encodeConfig);
    FIELD(NV_ENC_INITIALIZE_PARAMS, maxEncodeWidth);
    FIELD(NV_ENC_INITIALIZE_PARAMS, maxEncodeHeight);
    FIELD(NV_ENC_INITIALIZE_PARAMS, tuningInfo);

    printf("create_bitstream.size=%zu align=%zu\n", sizeof(NV_ENC_CREATE_BITSTREAM_BUFFER), _Alignof(NV_ENC_CREATE_BITSTREAM_BUFFER));
    FIELD(NV_ENC_CREATE_BITSTREAM_BUFFER, version);
    FIELD(NV_ENC_CREATE_BITSTREAM_BUFFER, bitstreamBuffer);

    printf("register.size=%zu align=%zu\n", sizeof(NV_ENC_REGISTER_RESOURCE), _Alignof(NV_ENC_REGISTER_RESOURCE));
    FIELD(NV_ENC_REGISTER_RESOURCE, version);
    FIELD(NV_ENC_REGISTER_RESOURCE, resourceType);
    FIELD(NV_ENC_REGISTER_RESOURCE, width);
    FIELD(NV_ENC_REGISTER_RESOURCE, height);
    FIELD(NV_ENC_REGISTER_RESOURCE, pitch);
    FIELD(NV_ENC_REGISTER_RESOURCE, subResourceIndex);
    FIELD(NV_ENC_REGISTER_RESOURCE, resourceToRegister);
    FIELD(NV_ENC_REGISTER_RESOURCE, registeredResource);
    FIELD(NV_ENC_REGISTER_RESOURCE, bufferFormat);
    FIELD(NV_ENC_REGISTER_RESOURCE, bufferUsage);

    printf("map.size=%zu align=%zu\n", sizeof(NV_ENC_MAP_INPUT_RESOURCE), _Alignof(NV_ENC_MAP_INPUT_RESOURCE));
    FIELD(NV_ENC_MAP_INPUT_RESOURCE, version);
    FIELD(NV_ENC_MAP_INPUT_RESOURCE, registeredResource);
    FIELD(NV_ENC_MAP_INPUT_RESOURCE, mappedResource);
    FIELD(NV_ENC_MAP_INPUT_RESOURCE, mappedBufferFmt);

    printf("pic.size=%zu align=%zu\n", sizeof(NV_ENC_PIC_PARAMS), _Alignof(NV_ENC_PIC_PARAMS));
    FIELD(NV_ENC_PIC_PARAMS, version);
    FIELD(NV_ENC_PIC_PARAMS, inputWidth);
    FIELD(NV_ENC_PIC_PARAMS, inputHeight);
    FIELD(NV_ENC_PIC_PARAMS, inputPitch);
    FIELD(NV_ENC_PIC_PARAMS, encodePicFlags);
    FIELD(NV_ENC_PIC_PARAMS, frameIdx);
    FIELD(NV_ENC_PIC_PARAMS, inputTimeStamp);
    FIELD(NV_ENC_PIC_PARAMS, inputDuration);
    FIELD(NV_ENC_PIC_PARAMS, inputBuffer);
    FIELD(NV_ENC_PIC_PARAMS, outputBitstream);
    FIELD(NV_ENC_PIC_PARAMS, completionEvent);
    FIELD(NV_ENC_PIC_PARAMS, bufferFmt);
    FIELD(NV_ENC_PIC_PARAMS, pictureStruct);

    printf("lock.size=%zu align=%zu\n", sizeof(NV_ENC_LOCK_BITSTREAM), _Alignof(NV_ENC_LOCK_BITSTREAM));
    FIELD(NV_ENC_LOCK_BITSTREAM, version);
    FIELD(NV_ENC_LOCK_BITSTREAM, outputBitstream);
    FIELD(NV_ENC_LOCK_BITSTREAM, frameIdx);
    FIELD(NV_ENC_LOCK_BITSTREAM, hwEncodeStatus);
    FIELD(NV_ENC_LOCK_BITSTREAM, bitstreamSizeInBytes);
    FIELD(NV_ENC_LOCK_BITSTREAM, outputTimeStamp);
    FIELD(NV_ENC_LOCK_BITSTREAM, outputDuration);
    FIELD(NV_ENC_LOCK_BITSTREAM, bitstreamBufferPtr);
    FIELD(NV_ENC_LOCK_BITSTREAM, pictureType);
    FIELD(NV_ENC_LOCK_BITSTREAM, frameAvgQP);

    printf("event.size=%zu align=%zu\n", sizeof(NV_ENC_EVENT_PARAMS), _Alignof(NV_ENC_EVENT_PARAMS));
    FIELD(NV_ENC_EVENT_PARAMS, version);
    FIELD(NV_ENC_EVENT_PARAMS, completionEvent);

    printf("versions config=0x%x rc=0x%x init=0x%x preset=0x%x create_bitstream=0x%x register=0x%x map=0x%x pic=0x%x lock=0x%x event=0x%x\n",
           NV_ENC_CONFIG_VER, NV_ENC_RC_PARAMS_VER, NV_ENC_INITIALIZE_PARAMS_VER,
           NV_ENC_PRESET_CONFIG_VER, NV_ENC_CREATE_BITSTREAM_BUFFER_VER,
           NV_ENC_REGISTER_RESOURCE_VER, NV_ENC_MAP_INPUT_RESOURCE_VER,
           NV_ENC_PIC_PARAMS_VER, NV_ENC_LOCK_BITSTREAM_VER, NV_ENC_EVENT_PARAMS_VER);
    printf("functions init=%zu create_bs=%zu destroy_bs=%zu encode=%zu lock=%zu unlock=%zu register_event=%zu unregister_event=%zu map=%zu unmap=%zu register_resource=%zu unregister_resource=%zu preset_ex=%zu\n",
           offsetof(NV_ENCODE_API_FUNCTION_LIST, nvEncInitializeEncoder),
           offsetof(NV_ENCODE_API_FUNCTION_LIST, nvEncCreateBitstreamBuffer),
           offsetof(NV_ENCODE_API_FUNCTION_LIST, nvEncDestroyBitstreamBuffer),
           offsetof(NV_ENCODE_API_FUNCTION_LIST, nvEncEncodePicture),
           offsetof(NV_ENCODE_API_FUNCTION_LIST, nvEncLockBitstream),
           offsetof(NV_ENCODE_API_FUNCTION_LIST, nvEncUnlockBitstream),
           offsetof(NV_ENCODE_API_FUNCTION_LIST, nvEncRegisterAsyncEvent),
           offsetof(NV_ENCODE_API_FUNCTION_LIST, nvEncUnregisterAsyncEvent),
           offsetof(NV_ENCODE_API_FUNCTION_LIST, nvEncMapInputResource),
           offsetof(NV_ENCODE_API_FUNCTION_LIST, nvEncUnmapInputResource),
           offsetof(NV_ENCODE_API_FUNCTION_LIST, nvEncRegisterResource),
           offsetof(NV_ENCODE_API_FUNCTION_LIST, nvEncUnregisterResource),
           offsetof(NV_ENCODE_API_FUNCTION_LIST, nvEncGetEncodePresetConfigEx));
    return 0;
}
