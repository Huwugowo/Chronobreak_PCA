# QueueBack media runtime: third-party notices

QueueBack invokes `ffmpeg.exe` and `ffprobe.exe` as separate programs. The
packaged runtime is built from the exact revisions recorded in
`runtime-manifest.json`; it is not the developer machine's PATH installation.

## FFmpeg and x264

FFmpeg 8.1.2 is built with GPL and version-3 components enabled and links the
GPL x264 encoder. The resulting programs are distributed under GNU GPL version
3 or later. `COPYING.GPLv3.txt` contains the license text. Corresponding source
and exact build instructions are identified in `SOURCE_AND_BUILD.md`.

## NVIDIA codec headers

The ffnvcodec headers are from nv-codec-headers tag `n12.2.72.0`. Their header
license permits use, modification, and redistribution under the MIT terms in
`NVCODEC_LICENSE.txt`. NVIDIA driver libraries are loaded from the operating
system at runtime and are not packaged by QueueBack.

## AMD AMF headers

The AMF headers are from AMD AMF tag `v1.4.36` and are available under the MIT
terms and standards notice in `AMF_LICENSE.txt`. AMD driver libraries are
loaded from the operating system at runtime and are not packaged by QueueBack.

## Intel oneVPL

The build uses Intel oneVPL 2.13.0 headers/static dispatcher from MSYS2 under
the MIT terms in `LIBVPL_LICENSE.txt`. Intel graphics driver components are not
packaged by QueueBack.

Codec patent or royalty obligations are separate from these software licenses.
This notice is informational and is not legal advice.
