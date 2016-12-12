extern crate jsonrpc_core;
extern crate futures;

use std::str::Lines;
use std::net::TcpStream;
use std::io::{Read, Write};
use self::jsonrpc_core::{IoHandler, Params, Value, Error};
use super::*;

fn serve_hosts(hosts: Vec<String>) -> Server {
	ServerBuilder::new(IoHandler::default())
		.cors(DomainsValidation::AllowOnly(vec![AccessControlAllowOrigin::Value("ethcore.io".into())]))
		.allowed_hosts(DomainsValidation::AllowOnly(hosts))
		.start_http(&"127.0.0.1:0".parse().unwrap())
		.unwrap()
}

fn serve() -> Server {
	use std::thread;
	let mut io = IoHandler::default();
	io.add_method("hello", |_params: Params| Ok(Value::String("world".into())));
	io.add_async_method("hello_async", |_params: Params| {
		futures::finished(Value::String("world".into())).boxed()
	});
	io.add_async_method("hello_async2", |_params: Params| {
		let (c, p) = futures::oneshot();
		thread::spawn(move || {
			thread::sleep(::std::time::Duration::from_millis(10));
			c.complete(Value::String("world".into()));
		});
		p.map_err(|_| Error::invalid_request()).boxed()
	});

	ServerBuilder::new(io)
		.cors(DomainsValidation::AllowOnly(vec![AccessControlAllowOrigin::Value("ethcore.io".into())]))
		.start_http(&"127.0.0.1:0".parse().unwrap())
		.unwrap()
}

struct Response {
	status: String,
	headers: String,
	body: String,
}

fn read_block(lines: &mut Lines) -> String {
	let mut block = String::new();
	loop {
		let line = lines.next();
		match line {
			Some("") | None => break,
			Some(v) => {
				block.push_str(v);
				block.push_str("\n");
			},
		}
	}
	block
}

fn request(server: Server, request: &str) -> Response {
	let mut req = TcpStream::connect(server.addrs()[0]).unwrap();
	req.write_all(request.as_bytes()).unwrap();

	let mut response = String::new();
	req.read_to_string(&mut response).unwrap();

	let mut lines = response.lines();
	let status = lines.next().unwrap().to_owned();
	let headers =	read_block(&mut lines);
	let body = read_block(&mut lines);

	Response {
		status: status,
		headers: headers,
		body: body,
	}
}

#[test]
fn should_return_method_not_allowed_for_get() {
	// given
	let server = serve();

	// when
	let response = request(server,
		"\
			GET / HTTP/1.1\r\n\
			Host: 127.0.0.1:8080\r\n\
			Connection: close\r\n\
			\r\n\
			I shouldn't be read.\r\n\
		"
	);

	// then
	assert_eq!(response.status, "HTTP/1.1 405 Method Not Allowed".to_owned());
	assert_eq!(response.body, "3D\nUsed HTTP Method is not allowed. POST or OPTIONS is required\n".to_owned());
}

#[test]
fn should_return_unsupported_media_type_if_not_json() {
	// given
	let server = serve();

	// when
	let response = request(server,
		"\
			POST / HTTP/1.1\r\n\
			Host: 127.0.0.1:8080\r\n\
			Connection: close\r\n\
			\r\n\
			{}\r\n\
		"
	);

	// then
	assert_eq!(response.status, "HTTP/1.1 415 Unsupported Media Type".to_owned());
	assert_eq!(response.body, "51\nSupplied content type is not allowed. Content-Type: application/json is required\n".to_owned());
}

#[test]
fn should_return_error_for_malformed_request() {
	// given
	let server = serve();

	// when
	let req = r#"{"jsonrpc":"3.0","method":"x"}"#;
	let response = request(server,
		&format!("\
			POST / HTTP/1.1\r\n\
			Host: 127.0.0.1:8080\r\n\
			Connection: close\r\n\
			Content-Type: application/json\r\n\
			Content-Length: {}\r\n\
			\r\n\
			{}\r\n\
		", req.as_bytes().len(), req)
	);

	// then
	assert_eq!(response.status, "HTTP/1.1 200 OK".to_owned());
	assert_eq!(response.body, invalid_request());
}

#[test]
fn should_return_error_for_malformed_request2() {
	// given
	let server = serve();

	// when
	let req = r#"{"jsonrpc":"2.0","metho1d":""}"#;
	let response = request(server,
		&format!("\
			POST / HTTP/1.1\r\n\
			Host: 127.0.0.1:8080\r\n\
			Connection: close\r\n\
			Content-Type: application/json\r\n\
			Content-Length: {}\r\n\
			\r\n\
			{}\r\n\
		", req.as_bytes().len(), req)
	);

	// then
	assert_eq!(response.status, "HTTP/1.1 200 OK".to_owned());
	assert_eq!(response.body, invalid_request());
}

#[test]
fn should_return_empty_response_for_notification() {
	// given
	let server = serve();

	// when
	let req = r#"{"jsonrpc":"2.0","method":"x"}"#;
	let response = request(server,
		&format!("\
			POST / HTTP/1.1\r\n\
			Host: 127.0.0.1:8080\r\n\
			Connection: close\r\n\
			Content-Type: application/json\r\n\
			Content-Length: {}\r\n\
			\r\n\
			{}\r\n\
		", req.as_bytes().len(), req)
	);

	// then
	assert_eq!(response.status, "HTTP/1.1 200 OK".to_owned());
	assert_eq!(response.body, "0\n".to_owned());
}


#[test]
fn should_return_method_not_found() {
	// given
	let server = serve();

	// when
	let req = r#"{"jsonrpc":"2.0","id":"1","method":"x"}"#;
	let response = request(server,
		&format!("\
			POST / HTTP/1.1\r\n\
			Host: 127.0.0.1:8080\r\n\
			Connection: close\r\n\
			Content-Type: application/json\r\n\
			Content-Length: {}\r\n\
			\r\n\
			{}\r\n\
		", req.as_bytes().len(), req)
	);

	// then
	assert_eq!(response.status, "HTTP/1.1 200 OK".to_owned());
	assert_eq!(response.body, method_not_found());
}

#[test]
fn should_add_cors_headers() {
	// given
	let server = serve();

	// when
	let req = r#"{"jsonrpc":"2.0","id":"1","method":"x"}"#;
	let response = request(server,
		&format!("\
			POST / HTTP/1.1\r\n\
			Host: 127.0.0.1:8080\r\n\
			Origin: ethcore.io\r\n\
			Connection: close\r\n\
			Content-Type: application/json\r\n\
			Content-Length: {}\r\n\
			\r\n\
			{}\r\n\
		", req.as_bytes().len(), req)
	);

	// then
	assert_eq!(response.status, "HTTP/1.1 200 OK".to_owned());
	assert_eq!(response.body, method_not_found());
	assert!(response.headers.contains("Access-Control-Allow-Origin: ethcore.io"), "Headers missing in {}", response.headers);
}

#[test]
fn should_not_add_cors_headers() {
	// given
	let server = serve();

	// when
	let req = r#"{"jsonrpc":"2.0","id":"1","method":"x"}"#;
	let response = request(server,
		&format!("\
			POST / HTTP/1.1\r\n\
			Host: 127.0.0.1:8080\r\n\
			Origin: fake.io\r\n\
			Connection: close\r\n\
			Content-Type: application/json\r\n\
			Content-Length: {}\r\n\
			\r\n\
			{}\r\n\
		", req.as_bytes().len(), req)
	);

	// then
	assert_eq!(response.status, "HTTP/1.1 200 OK".to_owned());
	assert_eq!(response.body, method_not_found());
}

#[test]
fn should_reject_invalid_hosts() {
	// given
	let server = serve_hosts(vec!["ethcore.io".into()]);

	// when
	let req = r#"{"jsonrpc":"2.0","id":"1","method":"x"}"#;
	let response = request(server,
		&format!("\
			POST / HTTP/1.1\r\n\
			Host: 127.0.0.1:8080\r\n\
			Connection: close\r\n\
			Content-Type: application/json\r\n\
			Content-Length: {}\r\n\
			\r\n\
			{}\r\n\
		", req.as_bytes().len(), req)
	);

	// then
	assert_eq!(response.status, "HTTP/1.1 403 Forbidden".to_owned());
	assert_eq!(response.body, invalid_host());
}

#[test]
fn should_reject_missing_host() {
	// given
	let server = serve_hosts(vec!["ethcore.io".into()]);

	// when
	let req = r#"{"jsonrpc":"2.0","id":"1","method":"x"}"#;
	let response = request(server,
		&format!("\
			POST / HTTP/1.1\r\n\
			Connection: close\r\n\
			Content-Type: application/json\r\n\
			Content-Length: {}\r\n\
			\r\n\
			{}\r\n\
		", req.as_bytes().len(), req)
	);

	// then
	assert_eq!(response.status, "HTTP/1.1 403 Forbidden".to_owned());
	assert_eq!(response.body, invalid_host());
}

#[test]
fn should_allow_if_host_is_valid() {
	// given
	let server = serve_hosts(vec!["ethcore.io".into()]);

	// when
	let req = r#"{"jsonrpc":"2.0","id":"1","method":"x"}"#;
	let response = request(server,
		&format!("\
			POST / HTTP/1.1\r\n\
			Host: ethcore.io\r\n\
			Connection: close\r\n\
			Content-Type: application/json\r\n\
			Content-Length: {}\r\n\
			\r\n\
			{}\r\n\
		", req.as_bytes().len(), req)
	);

	// then
	assert_eq!(response.status, "HTTP/1.1 200 OK".to_owned());
	assert_eq!(response.body, method_not_found());
}

#[test]
fn should_always_allow_the_bind_address() {
	// given
	let server = serve_hosts(vec!["ethcore.io".into()]);
	let addr = server.addrs()[0].clone();

	// when
	let req = r#"{"jsonrpc":"2.0","id":"1","method":"x"}"#;
	let response = request(server,
		&format!("\
			POST / HTTP/1.1\r\n\
			Host: {}\r\n\
			Connection: close\r\n\
			Content-Type: application/json\r\n\
			Content-Length: {}\r\n\
			\r\n\
			{}\r\n\
		", addr, req.as_bytes().len(), req)
	);

	// then
	assert_eq!(response.status, "HTTP/1.1 200 OK".to_owned());
	assert_eq!(response.body, method_not_found());
}

#[test]
fn should_always_allow_the_bind_address_as_localhost() {
	// given
	let server = serve_hosts(vec![]);
	let addr = server.addrs()[0].clone();

	// when
	let req = r#"{"jsonrpc":"2.0","id":"1","method":"x"}"#;
	let response = request(server,
		&format!("\
			POST / HTTP/1.1\r\n\
			Host: localhost:{}\r\n\
			Connection: close\r\n\
			Content-Type: application/json\r\n\
			Content-Length: {}\r\n\
			\r\n\
			{}\r\n\
		", addr.port(), req.as_bytes().len(), req)
	);

	// then
	assert_eq!(response.status, "HTTP/1.1 200 OK".to_owned());
	assert_eq!(response.body, method_not_found());
}

#[test]
fn should_handle_sync_requests_correctly() {
	// given
	let server = serve();
	let addr = server.addrs()[0].clone();

	// when
	let req = r#"{"jsonrpc":"2.0","id":"1","method":"hello"}"#;
	let response = request(server,
		&format!("\
			POST / HTTP/1.1\r\n\
			Host: localhost:{}\r\n\
			Connection: close\r\n\
			Content-Type: application/json\r\n\
			Content-Length: {}\r\n\
			\r\n\
			{}\r\n\
		", addr.port(), req.as_bytes().len(), req)
	);

	// then
	assert_eq!(response.status, "HTTP/1.1 200 OK".to_owned());
	assert_eq!(response.body, world());
}

#[test]
fn should_handle_async_requests_with_immediate_response_correctly() {
	// given
	let server = serve();
	let addr = server.addrs()[0].clone();

	// when
	let req = r#"{"jsonrpc":"2.0","id":"1","method":"hello_async"}"#;
	let response = request(server,
		&format!("\
			POST / HTTP/1.1\r\n\
			Host: localhost:{}\r\n\
			Connection: close\r\n\
			Content-Type: application/json\r\n\
			Content-Length: {}\r\n\
			\r\n\
			{}\r\n\
		", addr.port(), req.as_bytes().len(), req)
	);

	// then
	assert_eq!(response.status, "HTTP/1.1 200 OK".to_owned());
	assert_eq!(response.body, world());
}

#[test]
fn should_handle_async_requests_correctly() {
	// given
	let server = serve();
	let addr = server.addrs()[0].clone();

	// when
	let req = r#"{"jsonrpc":"2.0","id":"1","method":"hello_async2"}"#;
	let response = request(server,
		&format!("\
			POST / HTTP/1.1\r\n\
			Host: localhost:{}\r\n\
			Connection: close\r\n\
			Content-Type: application/json\r\n\
			Content-Length: {}\r\n\
			\r\n\
			{}\r\n\
		", addr.port(), req.as_bytes().len(), req)
	);

	// then
	assert_eq!(response.status, "HTTP/1.1 200 OK".to_owned());
	assert_eq!(response.body, world());
}

#[test]
fn should_handle_sync_batch_requests_correctly() {
	// given
	let server = serve();
	let addr = server.addrs()[0].clone();

	// when
	let req = r#"[{"jsonrpc":"2.0","id":"1","method":"hello"}]"#;
	let response = request(server,
		&format!("\
			POST / HTTP/1.1\r\n\
			Host: localhost:{}\r\n\
			Connection: close\r\n\
			Content-Type: application/json\r\n\
			Content-Length: {}\r\n\
			\r\n\
			{}\r\n\
		", addr.port(), req.as_bytes().len(), req)
	);

	// then
	assert_eq!(response.status, "HTTP/1.1 200 OK".to_owned());
	assert_eq!(response.body, world_batch());
}

fn invalid_host() -> String {
	"29\nProvided Host header is not whitelisted.\n".into()
}

fn method_not_found() -> String {
 "5A\n{\"jsonrpc\":\"2.0\",\"error\":{\"code\":-32601,\"message\":\"Method not found\",\"data\":null},\"id\":1}\n".into()
}

fn invalid_request() -> String {
 "5C\n{\"jsonrpc\":\"2.0\",\"error\":{\"code\":-32600,\"message\":\"Invalid request\",\"data\":null},\"id\":null}\n".into()
}
fn world() -> String {
 "2A\n{\"jsonrpc\":\"2.0\",\"result\":\"world\",\"id\":1}\n".into()
}
fn world_batch() -> String {
 "2C\n[{\"jsonrpc\":\"2.0\",\"result\":\"world\",\"id\":1}]\n".into()
}
