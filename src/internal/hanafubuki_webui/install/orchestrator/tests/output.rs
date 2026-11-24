use super::*;

#[test]
fn structured_output_preserves_streams_offsets_and_split_utf8_bytes() {
    let backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Exited { exit_code: 9 }],
    ));
    *backend.output.lock().unwrap() = vec![
        InstanceInstallOutputChunk {
            stream: InstanceInstallOutputStream::Stdout,
            data: vec![0xe2],
        },
        InstanceInstallOutputChunk {
            stream: InstanceInstallOutputStream::Stdout,
            data: vec![0x82, 0xac],
        },
        InstanceInstallOutputChunk {
            stream: InstanceInstallOutputStream::Stderr,
            data: b"!".to_vec(),
        },
    ];
    let service = InstanceInstallService::new(backend);
    let started = service.start(request(family_specs().remove(0).0)).unwrap();
    let failed = wait_terminal(&service, &started.operation_id);
    assert_eq!(failed.error.as_deref(), Some("€!"));
    let output = service.read_output(&started.operation_id, 0, None).unwrap();
    assert_eq!(output.start_offset, 0);
    assert_eq!(output.next_offset, 4);
    assert_eq!(output.chunks[0].stream, InstanceInstallOutputStream::Stdout);
    assert_eq!(output.chunks[0].data, vec![0xe2]);
    assert_eq!(output.chunks[1].data, vec![0x82, 0xac]);
    assert_eq!(output.chunks[2].stream, InstanceInstallOutputStream::Stderr);
}
