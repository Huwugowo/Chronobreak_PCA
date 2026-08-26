#include <stddef.h>
#include <stdio.h>
#include "nvEncodeAPI.h"

int main(void) {
    printf("function_list.size=%zu\n", sizeof(NV_ENCODE_API_FUNCTION_LIST));
    printf("function_list.guid_count=%zu\n", offsetof(NV_ENCODE_API_FUNCTION_LIST, nvEncGetEncodeGUIDCount));
    printf("function_list.guids=%zu\n", offsetof(NV_ENCODE_API_FUNCTION_LIST, nvEncGetEncodeGUIDs));
    printf("function_list.caps=%zu\n", offsetof(NV_ENCODE_API_FUNCTION_LIST, nvEncGetEncodeCaps));
    printf("function_list.destroy=%zu\n", offsetof(NV_ENCODE_API_FUNCTION_LIST, nvEncDestroyEncoder));
    printf("function_list.open_ex=%zu\n", offsetof(NV_ENCODE_API_FUNCTION_LIST, nvEncOpenEncodeSessionEx));
    printf("open_params.size=%zu\n", sizeof(NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS));
    printf("open_params.device=%zu\n", offsetof(NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS, device));
    printf("open_params.api_version=%zu\n", offsetof(NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS, apiVersion));
    printf("caps_params.size=%zu\n", sizeof(NV_ENC_CAPS_PARAM));
    printf("function_list.version=0x%08x\n", NV_ENCODE_API_FUNCTION_LIST_VER);
    printf("open_params.version=0x%08x\n", NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER);
    printf("caps_params.version=0x%08x\n", NV_ENC_CAPS_PARAM_VER);
    return 0;
}
