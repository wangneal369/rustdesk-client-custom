// Compile the actual app HTTP factory without requiring the video/capture toolchain.
#[path = "../../../src/hbbs_http/http_client.rs"]
#[allow(dead_code)]
mod http_client;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpListener};

#[tokio::test]
async fn audit_http_does_not_follow_redirects() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/audit", listener.local_addr().unwrap());
    let target = url.clone();
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        assert!(socket.read(&mut request).await.unwrap() > 0);
        let response = format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", target);
        socket.write_all(response.as_bytes()).await.unwrap();
    });
    let client = http_client::create_audit_http_client(true).unwrap();
    let response = client.post(url).body("audit-data").send().await.unwrap();
    assert_eq!(response.status().as_u16(), 307);
    task.await.unwrap();
}

#[tokio::test]
async fn remote_audit_factory_never_sends_plaintext() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/audit", listener.local_addr().unwrap());
    let client = http_client::create_audit_http_client(false).unwrap();
    assert!(client.post(url).body("audit-data").send().await.is_err());
    assert!(tokio::time::timeout(std::time::Duration::from_millis(50), listener.accept()).await.is_err());
}
