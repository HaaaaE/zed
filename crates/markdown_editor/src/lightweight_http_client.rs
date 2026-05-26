use std::{
    collections::HashMap,
    io::{Cursor, Read},
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::Duration,
};

use anyhow::{Context as _, anyhow};
use bytes::Bytes;
use futures::{FutureExt as _, channel::oneshot, future::BoxFuture};
use http_client::{
    AsyncBody, HttpClient, Inner, RedirectPolicy, Response, Url,
    http::{self, HeaderValue},
};
use parking_lot::{Condvar, Mutex};
use ureq::RequestExt as _;

const DOWNLOAD_WORKERS: usize = 4;
const MAX_DOWNLOADS_PER_HOST: usize = 2;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RESPONSE_BYTES: u64 = 20 * 1024 * 1024;

pub(crate) struct MarkdownHttpClient {
    user_agent: HeaderValue,
    proxy: Option<Url>,
    requests: Sender<DownloadJob>,
}

struct DownloadJob {
    request: http::Request<RequestBody>,
    host_key: String,
    response: oneshot::Sender<anyhow::Result<Response<AsyncBody>>>,
}

enum RequestBody {
    Empty,
    Bytes(Vec<u8>),
}

#[derive(Default)]
struct DownloadLimits {
    hosts: Mutex<HashMap<String, Arc<HostLimit>>>,
}

#[derive(Default)]
struct HostLimit {
    active: Mutex<usize>,
    changed: Condvar,
}

struct HostLimitPermit {
    limit: Arc<HostLimit>,
}

impl MarkdownHttpClient {
    pub(crate) fn new(user_agent: &str) -> anyhow::Result<Self> {
        let user_agent = HeaderValue::from_str(user_agent)?;
        let proxy = http_client::read_proxy_from_env();
        let mut config = ureq::Agent::config_builder()
            .user_agent(user_agent.to_str()?)
            .timeout_global(Some(REQUEST_TIMEOUT))
            .http_status_as_error(false);

        if let Some(proxy) = ureq::Proxy::try_from_env() {
            config = config.proxy(Some(proxy));
        }

        let agent = config.build().new_agent();
        let (sender, receiver) = mpsc::channel();
        let receiver = Arc::new(Mutex::new(receiver));
        let limits = Arc::new(DownloadLimits::default());

        for worker_index in 0..DOWNLOAD_WORKERS {
            let agent = agent.clone();
            let receiver = receiver.clone();
            let limits = limits.clone();
            thread::Builder::new()
                .name(format!("markdown-http-{worker_index}"))
                .spawn(move || worker_loop(agent, receiver, limits))
                .context("failed to spawn markdown HTTP worker")?;
        }

        Ok(Self {
            user_agent,
            proxy,
            requests: sender,
        })
    }
}

impl HttpClient for MarkdownHttpClient {
    fn user_agent(&self) -> Option<&HeaderValue> {
        Some(&self.user_agent)
    }

    fn proxy(&self) -> Option<&Url> {
        self.proxy.as_ref()
    }

    fn send(
        &self,
        req: http::Request<AsyncBody>,
    ) -> BoxFuture<'static, anyhow::Result<Response<AsyncBody>>> {
        let request = match prepare_request(req) {
            Ok(request) => request,
            Err(error) => return async move { Err(error) }.boxed(),
        };
        let host_key = host_key(request.uri());
        let (sender, receiver) = oneshot::channel();

        if self
            .requests
            .send(DownloadJob {
                request,
                host_key,
                response: sender,
            })
            .is_err()
        {
            return async { Err(anyhow!("markdown HTTP workers are no longer running")) }.boxed();
        }

        async move {
            receiver
                .await
                .map_err(|_| anyhow!("markdown HTTP request was canceled before completion"))?
        }
        .boxed()
    }
}

fn prepare_request(req: http::Request<AsyncBody>) -> anyhow::Result<http::Request<RequestBody>> {
    let (parts, body) = req.into_parts();
    let body = match body.0 {
        Inner::Empty => RequestBody::Empty,
        Inner::Bytes(mut cursor) => {
            let mut bytes = Vec::new();
            cursor.read_to_end(&mut bytes)?;
            RequestBody::Bytes(bytes)
        }
        Inner::AsyncReader(_) => {
            return Err(anyhow!(
                "markdown HTTP client does not support streaming request bodies"
            ));
        }
    };

    Ok(http::Request::from_parts(parts, body))
}

fn worker_loop(
    agent: ureq::Agent,
    receiver: Arc<Mutex<Receiver<DownloadJob>>>,
    limits: Arc<DownloadLimits>,
) {
    loop {
        let job = {
            let receiver = receiver.lock();
            receiver.recv()
        };
        let Ok(job) = job else {
            break;
        };

        let host_permit = limits.acquire_host(job.host_key);
        let result = send_request(&agent, job.request);
        drop(host_permit);

        if job.response.send(result).is_err() {
            continue;
        }
    }
}

fn send_request(
    agent: &ureq::Agent,
    request: http::Request<RequestBody>,
) -> anyhow::Result<Response<AsyncBody>> {
    let (mut parts, body) = request.into_parts();
    let redirects = parts
        .extensions
        .remove::<RedirectPolicy>()
        .unwrap_or_default();
    let max_redirects = match redirects {
        RedirectPolicy::NoFollow => 0,
        RedirectPolicy::FollowLimit(limit) => limit,
        RedirectPolicy::FollowAll => 100,
    };

    let body = match body {
        RequestBody::Empty => ureq::SendBody::none(),
        RequestBody::Bytes(bytes) => ureq::SendBody::from_owned_reader(Cursor::new(bytes)),
    };
    let request = http::Request::from_parts(parts, body);
    let mut response = request
        .with_agent(agent)
        .configure()
        .max_redirects(max_redirects)
        .max_redirects_will_error(false)
        .http_status_as_error(false)
        .run()?;

    let status = response.status();
    let version = response.version();
    let headers = response.headers().clone();
    let bytes = response
        .body_mut()
        .with_config()
        .limit(MAX_RESPONSE_BYTES)
        .read_to_vec()?;

    let mut builder = http::Response::builder().status(status).version(version);
    *builder.headers_mut().unwrap() = headers;
    builder
        .body(AsyncBody::from_bytes(Bytes::from(bytes)))
        .map_err(Into::into)
}

fn host_key(uri: &http::Uri) -> String {
    uri.authority()
        .map(|authority| authority.as_str().to_ascii_lowercase())
        .unwrap_or_default()
}

impl DownloadLimits {
    fn acquire_host(&self, host_key: String) -> HostLimitPermit {
        let limit = {
            let mut hosts = self.hosts.lock();
            hosts.entry(host_key).or_default().clone()
        };
        let mut active = limit.active.lock();
        while *active >= MAX_DOWNLOADS_PER_HOST {
            limit.changed.wait(&mut active);
        }
        *active += 1;
        drop(active);

        HostLimitPermit { limit }
    }
}

impl Drop for HostLimitPermit {
    fn drop(&mut self) {
        let mut active = self.limit.active.lock();
        *active = active.saturating_sub(1);
        self.limit.changed.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use futures::executor::block_on;

    use super::*;

    #[test]
    fn returns_success_status_headers_and_body() {
        let server = TestServer::spawn(|_| TestResponse {
            status: 200,
            headers: vec![("X-Test", "ok")],
            body: b"hello".to_vec(),
        });
        let response = block_on(client().get(&server.url("/cat.png"), AsyncBody::empty(), true))
            .expect("request failed");

        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["x-test"], "ok");
        assert_eq!(read_body(response), b"hello");
    }

    #[test]
    fn returns_http_error_status_as_response() {
        let server = TestServer::spawn(|_| TestResponse {
            status: 404,
            headers: vec![],
            body: b"missing".to_vec(),
        });
        let response =
            block_on(client().get(&server.url("/missing.png"), AsyncBody::empty(), true))
                .expect("404 should be an HTTP response");

        assert_eq!(response.status(), 404);
        assert_eq!(read_body(response), b"missing");
    }

    #[test]
    fn enforces_response_body_limit() {
        let server = TestServer::spawn(|_| TestResponse {
            status: 200,
            headers: vec![],
            body: vec![b'x'; (MAX_RESPONSE_BYTES as usize) + 1],
        });
        let error = block_on(client().get(&server.url("/large.bin"), AsyncBody::empty(), true))
            .err()
            .expect("oversized response should fail");

        assert!(error.to_string().contains("larger than request limit"));
    }

    #[test]
    fn queues_requests_with_global_worker_limit() {
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let server = {
            let active = active.clone();
            let max_active = max_active.clone();
            TestServer::spawn(move |_| {
                let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                max_active.fetch_max(now, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(10));
                active.fetch_sub(1, Ordering::SeqCst);
                TestResponse {
                    status: 200,
                    headers: vec![],
                    body: b"ok".to_vec(),
                }
            })
        };
        let client = client();
        block_on(futures::future::join_all((0..100).map(|index| {
            client.get(
                &server.url(&format!("/image-{index}.png")),
                AsyncBody::empty(),
                true,
            )
        })))
        .into_iter()
        .for_each(|response| {
            response.expect("request failed");
        });

        assert!(max_active.load(Ordering::SeqCst) <= DOWNLOAD_WORKERS);
    }

    #[test]
    fn limits_same_host_requests() {
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let server = {
            let active = active.clone();
            let max_active = max_active.clone();
            TestServer::spawn(move |_| {
                let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                max_active.fetch_max(now, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(10));
                active.fetch_sub(1, Ordering::SeqCst);
                TestResponse {
                    status: 200,
                    headers: vec![],
                    body: b"ok".to_vec(),
                }
            })
        };
        let client = client();
        block_on(futures::future::join_all((0..20).map(|index| {
            client.get(
                &server.url(&format!("/image-{index}.png")),
                AsyncBody::empty(),
                true,
            )
        })))
        .into_iter()
        .for_each(|response| {
            response.expect("request failed");
        });

        assert!(max_active.load(Ordering::SeqCst) <= MAX_DOWNLOADS_PER_HOST);
    }

    #[test]
    fn async_reader_request_body_is_unsupported() {
        let request = http::Request::builder()
            .uri("http://example.test/upload")
            .method(http::Method::POST)
            .body(AsyncBody::from_reader(futures::io::Cursor::new(vec![
                1, 2, 3,
            ])))
            .unwrap();
        let error = block_on(client().send(request))
            .err()
            .expect("streaming body should fail");

        assert!(error.to_string().contains("streaming request bodies"));
    }

    #[test]
    fn dropped_future_does_not_panic_when_worker_completes() {
        let server = TestServer::spawn(|_| {
            std::thread::sleep(Duration::from_millis(10));
            TestResponse {
                status: 200,
                headers: vec![],
                body: b"ok".to_vec(),
            }
        });
        let future = client().get(&server.url("/slow.png"), AsyncBody::empty(), true);
        drop(future);

        std::thread::sleep(Duration::from_millis(50));
    }

    fn client() -> MarkdownHttpClient {
        MarkdownHttpClient::new("markdown-editor/test").unwrap()
    }

    fn read_body(response: Response<AsyncBody>) -> Vec<u8> {
        let (_, mut body) = response.into_parts();
        let mut bytes = Vec::new();
        block_on(futures::AsyncReadExt::read_to_end(&mut body, &mut bytes)).unwrap();
        bytes
    }

    struct TestServer {
        address: std::net::SocketAddr,
    }

    struct TestResponse {
        status: u16,
        headers: Vec<(&'static str, &'static str)>,
        body: Vec<u8>,
    }

    impl TestServer {
        fn spawn(handler: impl Fn(String) -> TestResponse + Send + Sync + 'static) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let handler = Arc::new(handler);

            thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else {
                        break;
                    };
                    let handler = handler.clone();
                    thread::spawn(move || serve_connection(stream, handler));
                }
            });

            Self { address }
        }

        fn url(&self, path: &str) -> String {
            format!("http://{}{}", self.address, path)
        }
    }

    fn serve_connection(
        mut stream: TcpStream,
        handler: Arc<dyn Fn(String) -> TestResponse + Send + Sync>,
    ) {
        let mut request = Vec::new();
        let mut buffer = [0; 1024];
        loop {
            let Ok(read) = stream.read(&mut buffer) else {
                return;
            };
            if read == 0 {
                return;
            }
            request.extend_from_slice(&buffer[..read]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }

        let path = String::from_utf8_lossy(&request)
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/")
            .to_string();
        let response = handler(path);
        let reason = match response.status {
            200 => "OK",
            404 => "Not Found",
            _ => "OK",
        };
        let mut headers = format!(
            "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n",
            response.status,
            reason,
            response.body.len()
        );
        for (name, value) in response.headers {
            headers.push_str(name);
            headers.push_str(": ");
            headers.push_str(value);
            headers.push_str("\r\n");
        }
        headers.push_str("\r\n");

        stream.write_all(headers.as_bytes()).unwrap();
        stream.write_all(&response.body).unwrap();
    }
}
