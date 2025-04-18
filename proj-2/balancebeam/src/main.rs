mod request;
mod response;

use clap::Parser;
use tokio::task;
use tokio::sync::RwLock;
use rand::{Rng, SeedableRng};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::sleep;

/// Contains information parsed from the command-line invocation of balancebeam. The Clap macros
/// provide a fancy way to automatically construct a command-line argument parser.
#[derive(Parser, Debug)]
#[command(about = "Fun with load balancing")]
struct CmdOptions {
    /// "IP/port to bind to"
    #[arg(short, long, default_value = "0.0.0.0:1100")]
    bind: String,
    /// "Upstream host to forward requests to"
    #[arg(short, long)]
    upstream: Vec<String>,
    /// "Perform active health checks on this interval (in seconds)"
    #[arg(long, default_value = "10")]
    active_health_check_interval: usize,
     /// "Path to send request to for active health checks"
    #[arg(long, default_value = "/")]
    active_health_check_path: String,
    /// "Maximum number of requests to accept per IP per minute (0 = unlimited)"
    #[arg(long, default_value = "0")]
    max_requests_per_minute: usize,
}

/// Contains information about the state of balancebeam (e.g. what servers we are currently proxying
/// to, what servers have failed, rate limiting counts, etc.)
///
/// You should add fields to this struct in later milestones.
#[derive(Debug, Clone)]
struct ProxyState {
    /// How frequently we check whether upstream servers are alive (Milestone 4)
    active_health_check_interval: usize,
    /// Where we should send requests when doing active health checks (Milestone 4)
    active_health_check_path: String,
    /// Maximum number of requests an individual IP can make in a minute (Milestone 5)
    max_requests_per_minute: usize,
    // Record how many times the client request
    rate_limiting_table: Arc<RwLock<HashMap<String, usize>>>,
    /// Addresses of servers that we are proxying to
    upstream_addresses: Vec<String>,
    /// Addresses of active servers taht we are proxying to
    active_addresses: Arc<RwLock<Vec<String>>>,
}
#[tokio::main]
async fn main() {
    // Initialize the logging library. You can print log messages using the `log` macros:
    // https://docs.rs/log/0.4.8/log/ You are welcome to continue using print! statements; this
    // just looks a little prettier.
    if let Err(_) = std::env::var("RUST_LOG") {
        std::env::set_var("RUST_LOG", "debug");
    }
    pretty_env_logger::init();

    // Parse the command line arguments passed to this program
    let options = CmdOptions::parse();
    if options.upstream.len() < 1 {
        log::error!("At least one upstream server must be specified using the --upstream option.");
        std::process::exit(1);
    }

    // Start listening for connections
    let listener = match TcpListener::bind(&options.bind).await {
        Ok(listener) => listener,
        Err(err) => {
            log::error!("Could not bind to {}: {}", options.bind, err);
            std::process::exit(1);
        }
    };
    log::info!("Listening for requests on {}", options.bind);

    // Handle incoming connections
    let state = ProxyState {
        active_addresses: Arc::new(RwLock::new(options.upstream.clone())),
        upstream_addresses: options.upstream,
        active_health_check_interval: options.active_health_check_interval,
        active_health_check_path: options.active_health_check_path,
        max_requests_per_minute: options.max_requests_per_minute,
        rate_limiting_table: Arc::new(RwLock::new(HashMap::new())),
    };
    
    // For active health check
    let state_copy_0 = state.clone();
    task::spawn(async move {
        active_health_check(&state_copy_0).await;
    });

    // For Rate limiting
    let state_copy_1 = state.clone();
    task::spawn(async move {
        times_clear(&state_copy_1).await;
    });


    while let Ok((stream, _)) = listener.accept().await {
        // Handle the connection!
        let state_copy = state.clone();
        task::spawn(async move {
            handle_connection(stream, &state_copy).await;
        });
    }
}

async fn times_clear(state: &ProxyState) {
    loop {
        sleep(Duration::from_secs(60)).await;
        let mut table = state.rate_limiting_table.write().await;
        table.values_mut().for_each(|value| *value = 0);
        drop(table);
    }
}

async fn active_health_check(state: &ProxyState) {
    loop {
        sleep(Duration::from_secs(state.active_health_check_interval as u64)).await;
        log::info!("Active health check interval begin");
        let mut active_upstream = state.active_addresses.write().await;
        // clear the active server
        active_upstream.clear();
        for upstream in &state.upstream_addresses {
            // connect upstream server
            let request = http::Request::builder()
                        .method(http::Method::GET)
                        .uri(&state.active_health_check_path)
                        .header("Host", upstream.clone())
                        .body(Vec::new())
                        .unwrap();
            match TcpStream::connect(upstream).await {
                Ok(mut tcp_stream) => {
                    // log::info!("Active_health_checks: Successfully connect to upstream {}", &upstream);
                    if let Err(_) = request::write_to_stream(&request, &mut tcp_stream).await {
                        // fail to write the request into tcp_stream
                        log::info!("Active_health_checks: Failed to write the request into tcp_stream {:?}", tcp_stream);
                        return;
                    }

                    let response = match response::read_from_stream(&mut tcp_stream, request.method()).await {
                        Ok(response) => response,
                        Err(error) => {
                            log::error!("Error reading response from server: {:?}", error);
                            return;
                        }
                    };
                    if response.status().as_u16() == 200 {
                        active_upstream.push(upstream.clone());
                    }
                },
                Err(_) => {
                    // fail to connect the upstream server
                    log::info!("Active_health_checks: Failed to connect to upstream {}", upstream);
                    return;
                },
            }
        }
        // Need to unlock!!!
        drop(active_upstream);
    }
}


async fn connect_to_upstream(state: &ProxyState) -> Result<TcpStream, std::io::Error> {
    let mut rng = rand::rngs::StdRng::from_entropy();
    let shared_active_addresses = Arc::clone(&state.active_addresses);
    let active_addresses = shared_active_addresses.read().await;
    // let active_addresses = &state.upstream_addresses;
    let len = active_addresses.len();
    let mut upstream_idx = rng.gen_range(0..len);
    let record_idx = upstream_idx;
    let mut error = None;
    loop {
        let upstream_ip = &active_addresses[upstream_idx];
        match TcpStream::connect(upstream_ip).await {
            Ok(tcp_stream) => {
                log::info!("Successfully connect to upstream {}", upstream_ip);
                return Ok(tcp_stream);
            },
            Err(err) => {
                error = Some(err);
            },
        }
        upstream_idx = (upstream_idx + 1) % len;
        if upstream_idx == record_idx {
            break;
        }
    }
    return Err(error.unwrap());
}

async fn send_response(client_conn: &mut TcpStream, response: &http::Response<Vec<u8>>) {
    let client_ip = client_conn.peer_addr().unwrap().ip().to_string();
    log::info!(
        "{} <- {}",
        client_ip,
        response::format_response_line(&response)
    );
    if let Err(error) = response::write_to_stream(&response, client_conn).await {
        log::warn!("Failed to send response to client: {}", error);
        return;
    }
}

async fn handle_connection(mut client_conn: TcpStream, state: &ProxyState) {
    let client_ip = client_conn.peer_addr().unwrap().ip().to_string();
    log::info!("Connection received from {}", client_ip);

    // Open a connection to a random destination server
    let mut upstream_conn = match connect_to_upstream(state).await {
        Ok(stream) => stream,
        Err(_error) => {
            let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
            send_response(&mut client_conn, &response).await;
            return;
        }
    };
    let upstream_ip = upstream_conn.peer_addr().unwrap().ip().to_string();

    // The client may now send us one or more requests. Keep trying to read requests until the
    // client hangs up or we get an error.
    loop {
        // Read a request from the client
        let mut request = match request::read_from_stream(&mut client_conn).await {
            Ok(request) => {
                if state.max_requests_per_minute == 0 {
                    request
                } else {
                    let mut table = state.rate_limiting_table.write().await;
                    let entry = table.entry(client_ip.clone()).or_insert(0);
                
                    if *entry >= state.max_requests_per_minute {
                        let response = response::make_http_error(http::StatusCode::TOO_MANY_REQUESTS);
                        send_response(&mut client_conn, &response).await;
                        drop(table); 
                        continue;
                    } else {
                        *entry += 1;
                        drop(table); 
                        request
                    }
                }
            },
            // Handle case where client closed connection and is no longer sending requests
            Err(request::Error::IncompleteRequest(0)) => {
                log::debug!("Client finished sending requests. Shutting down connection");
                return;
            }
            // Handle I/O error in reading from the client
            Err(request::Error::ConnectionError(io_err)) => {
                log::info!("Error reading request from client stream: {}", io_err);
                return;
            }
            Err(error) => {
                log::debug!("Error parsing request: {:?}", error);
                let response = response::make_http_error(match error {
                    request::Error::IncompleteRequest(_)
                    | request::Error::MalformedRequest(_)
                    | request::Error::InvalidContentLength
                    | request::Error::ContentLengthMismatch => http::StatusCode::BAD_REQUEST,
                    request::Error::RequestBodyTooLarge => http::StatusCode::PAYLOAD_TOO_LARGE,
                    request::Error::ConnectionError(_) => http::StatusCode::SERVICE_UNAVAILABLE,
                });
                send_response(&mut client_conn, &response).await;
                continue;
            }
        };
        log::info!(
            "{} -> {}: {}",
            client_ip,
            upstream_ip,
            request::format_request_line(&request)
        );

        // Add X-Forwarded-For header so that the upstream server knows the client's IP address.
        // (We're the ones connecting directly to the upstream server, so without this header, the
        // upstream server will only know our IP, not the client's.)
        request::extend_header_value(&mut request, "x-forwarded-for", &client_ip);

        // Forward the request to the server
        if let Err(error) = request::write_to_stream(&request, &mut upstream_conn).await {
            log::error!(
                "Failed to send request to upstream {}: {}",
                upstream_ip,
                error
            );
            let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
            send_response(&mut client_conn, &response).await;
            return;
        }
        log::debug!("Forwarded request to server");

        // Read the server's response
        let response = match response::read_from_stream(&mut upstream_conn, request.method()).await {
            Ok(response) => response,
            Err(error) => {
                log::error!("Error reading response from server: {:?}", error);
                let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
                send_response(&mut client_conn, &response).await;
                return;
            }
        };
        // Forward the response to the client
        send_response(&mut client_conn, &response).await;
        log::debug!("Forwarded response to client");
    }
}
