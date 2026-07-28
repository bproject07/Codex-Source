use super::health_url_from_cmdline_bytes;
use super::parse_fully_stopped;

#[test]
fn health_url_uses_loopback_for_wildcard_service_listen() {
    assert_eq!(
        health_url_from_cmdline_bytes(
            b"/opt/codex-raw/current/codex-raw\x00api-server\x00--listen\x000.0.0.0:8080\x00"
        )
        .expect("IPv4 health URL"),
        "http://127.0.0.1:8080/healthz"
    );
    assert_eq!(
        health_url_from_cmdline_bytes(
            b"/opt/codex-raw/current/codex-raw\x00api-server\x00--listen=[::]:9090\x00"
        )
        .expect("IPv6 health URL"),
        "http://[::1]:9090/healthz"
    );
}

#[test]
fn service_cmdline_requires_api_server_and_uses_safe_default() {
    assert_eq!(
        health_url_from_cmdline_bytes(b"codex-raw\x00api-server\x00").expect("default health URL"),
        "http://127.0.0.1:8080/healthz"
    );
    assert!(health_url_from_cmdline_bytes(b"codex-raw\x00send\x00hello\x00").is_err());
}

#[test]
fn deactivating_service_is_not_mistaken_for_stopped() {
    assert!(
        !parse_fully_stopped("ActiveState=deactivating\nSubState=stop-sigterm\nMainPID=42\n")
            .expect("parse transitional state")
    );
    assert!(
        parse_fully_stopped("ActiveState=inactive\nSubState=dead\nMainPID=0\n")
            .expect("parse stopped state")
    );
}
