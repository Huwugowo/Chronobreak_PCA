use std::path::PathBuf;

#[tokio::test]
#[ignore = "requires the explicitly materialized pinned Windows runtime"]
async fn packaged_runtime_matches_the_embedded_contract() {
    let root = std::env::var_os("QUEUEBACK_TEST_MEDIA_RUNTIME")
        .map(PathBuf::from)
        .expect("set QUEUEBACK_TEST_MEDIA_RUNTIME to the prepared runtime root");
    let tools = queueback_media_runtime::resolve_root(&root).await.unwrap();

    assert_eq!(
        tools.runtime_id(),
        "queueback-ffmpeg-8.1.2-windows-x86_64-r6"
    );
    assert!(tools.ffmpeg().starts_with(tools.root()));
    assert!(tools.ffprobe().starts_with(tools.root()));
    assert!(tools.capabilities().filters.contains("gfxcapture"));
    assert!(tools.capabilities().filters.contains("scale_d3d11"));
    for encoder in [
        "h264_nvenc",
        "hevc_nvenc",
        "h264_amf",
        "hevc_amf",
        "h264_qsv",
        "hevc_qsv",
    ] {
        assert!(tools.capabilities().encoders.contains(encoder));
    }
}
