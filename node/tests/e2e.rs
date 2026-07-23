//! End-to-end: serve the real node router in-process against a recording
//! backend, then drive it with the production `NodeBackend` HTTP client. This
//! exercises the whole client<->node contract (routing, auth, JSON bodies).

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use wg_backend::{InterfaceSpec, PeerSpec, PeerStatus, Result, WireguardBackend};

#[derive(Default)]
struct Recorder {
    applied: Mutex<Vec<(String, usize)>>, // (iface name, peer count)
    removed: Mutex<Vec<String>>,
}

#[async_trait]
impl WireguardBackend for Recorder {
    async fn apply(&self, iface: &InterfaceSpec, peers: &[PeerSpec]) -> Result<()> {
        self.applied.lock().unwrap().push((iface.name.clone(), peers.len()));
        Ok(())
    }
    async fn status(&self, iface_name: &str) -> Result<Vec<PeerStatus>> {
        Ok(vec![PeerStatus { public_key: format!("PUB-{iface_name}"), endpoint: None, last_handshake: Some(42) }])
    }
    async fn remove(&self, _iface_name: &str) -> Result<()> {
        self.removed.lock().unwrap().push(_iface_name.to_string());
        Ok(())
    }
}

async fn serve(backend: Arc<Recorder>, key: &str) -> String {
    let app = wg_vpng_node::app(backend, Arc::from(key));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn iface() -> InterfaceSpec {
    InterfaceSpec {
        name: "wg0".into(),
        listen_port: 51820,
        address: "10.8.0.1/24".into(),
        private_key: "SRVPRIV".into(),
    }
}

#[tokio::test]
async fn apply_status_remove_roundtrip() {
    let rec = Arc::new(Recorder::default());
    let url = serve(rec.clone(), "secret-key").await;
    let client = wg_backend::NodeBackend::new(&url, "secret-key");

    let peers = vec![PeerSpec { public_key: "P1".into(), address: "10.8.0.2/32".into(), preshared_key: None }];
    client.apply(&iface(), &peers).await.unwrap();

    let st = client.status("wg0").await.unwrap();
    assert_eq!(st.len(), 1);
    assert_eq!(st[0].public_key, "PUB-wg0");
    assert_eq!(st[0].last_handshake, Some(42));

    client.remove("wg0").await.unwrap();

    assert_eq!(&*rec.applied.lock().unwrap(), &[("wg0".to_string(), 1)]);
    assert_eq!(&*rec.removed.lock().unwrap(), &["wg0".to_string()]);
}

#[tokio::test]
async fn wrong_key_is_rejected() {
    let rec = Arc::new(Recorder::default());
    let url = serve(rec.clone(), "right-key").await;
    let client = wg_backend::NodeBackend::new(&url, "wrong-key");

    let err = client.apply(&iface(), &[]).await.unwrap_err();
    assert!(err.to_string().contains("401"), "expected 401, got: {err}");
    assert!(rec.applied.lock().unwrap().is_empty());
}
