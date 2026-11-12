// Standalone gRPC server for the manual checks in docs/manual-tests/rok-grpc.md.
// It serves the same Greeter fixture as the transport tests.
//
// Usage: test_grpc_server [--port <n>] [--reflection none|v1|v1alpha]
use rocket_infra::grpc::test_server::{start_on, Reflection};

const USAGE: &str = "usage: test_grpc_server [--port <n>] [--reflection none|v1|v1alpha]";

fn parse_args() -> Result<(u16, Reflection), String> {
    let mut port = 50051;
    let mut reflection = Reflection::V1;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("{flag} needs a value\n{USAGE}"))?;
        match flag.as_str() {
            "--port" => {
                port = value
                    .parse()
                    .map_err(|_| format!("bad port: {value}\n{USAGE}"))?;
            }
            "--reflection" => {
                reflection = match value.as_str() {
                    "none" => Reflection::None,
                    "v1" => Reflection::V1,
                    "v1alpha" => Reflection::V1Alpha,
                    _ => return Err(format!("bad reflection: {value}\n{USAGE}")),
                };
            }
            _ => return Err(format!("unknown flag: {flag}\n{USAGE}")),
        }
    }
    Ok((port, reflection))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (port, reflection) = parse_args()?;
    let server = start_on(port, None, reflection).await?;
    println!("listening on 127.0.0.1:{}", server.addr.port());
    tokio::signal::ctrl_c().await?;
    Ok(())
}
