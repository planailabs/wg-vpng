//! Integration test against the in-repo fake RouterOS server: drive the real
//! Client through ensure_interface + reconcile_addresses and assert the
//! dual-stack addresses land (tagged with the wg-vpng comment).

use std::net::TcpStream;
use std::process::Command;
use std::time::Duration;

fn wait_port(port: u16) {
    for _ in 0..100 {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("fake-mikrotik did not open port {port}");
}

#[tokio::test]
async fn reconcile_assigns_dual_stack_addresses() {
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fake-mikrotik.py");
    let mut child = Command::new("python3").arg(script).arg("18099").spawn().expect("spawn fake");
    wait_port(18099);

    let c = mikrotik_api::Client::new("http://127.0.0.1:18099", "admin", "testpass", false).unwrap();
    c.ensure_interface("wg0", 51820, "PRIV").await.unwrap();
    c.reconcile_addresses("wg0", &["10.8.0.1/24".into(), "fd00:8::1/64".into()]).await.unwrap();

    let v4 = c.list_addresses(false, "wg0").await.unwrap();
    let v6 = c.list_addresses(true, "wg0").await.unwrap();

    // Reconciling to a new set removes the stale address.
    c.reconcile_addresses("wg0", &["10.8.0.1/24".into()]).await.unwrap();
    let v6b = c.list_addresses(true, "wg0").await.unwrap();
    let _ = child.kill();

    assert!(v4.iter().any(|a| a.address == "10.8.0.1/24" && a.comment == "wg-vpng:wg0"), "v4: {v4:?}");
    assert!(v6.iter().any(|a| a.address == "fd00:8::1/64" && a.comment == "wg-vpng:wg0"), "v6: {v6:?}");
    assert!(v6b.is_empty(), "stale v6 not removed: {v6b:?}");
}
