use ark_std::{start_timer, end_timer};
use chorus::network::{download_from_network, upload_to_network, ClientDownloadHandover, NetworkRequest, API};
use std::env;

fn load_config() -> serde_json::Value {
    let config_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("config.json");
    let data = std::fs::read_to_string(&config_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", config_path.display(), e));
    serde_json::from_str(&data)
        .unwrap_or_else(|e| panic!("Failed to parse {}: {}", config_path.display(), e))
}

fn parse_num_clients(s: &str) -> usize {
    match s.trim().to_uppercase().as_str() {
        "1M" => 10usize.pow(6),
        "10M" => 10usize.pow(7),
        "100M" => 10usize.pow(8),
        other => other.parse::<usize>().expect("Invalid NUM_CLIENTS value"),
    }
}

#[tokio::main]
async fn main() {
    let config = load_config();

    let server_ip = env::var("SERVER_IP")
        .expect("SERVER_IP must be set to the server VM's IP address");

    let case: usize = env::var("BENCH_CASES")
        .map(|v| v.trim().parse().expect("BENCH_CASES must be a number"))
        .unwrap_or_else(|_| {
            config["bench_cases"][0]["case"].as_u64()
                .expect("config.json bench_cases[0].case missing") as usize
        });

    let num_clients: usize = env::var("NUM_CLIENTS")
        .map(|v| parse_num_clients(&v))
        .unwrap_or_else(|_| {
            let s = config["num_clients"][0].as_str()
                .expect("config.json num_clients[0] missing");
            parse_num_clients(s)
        });

    let request = NetworkRequest {
        case,
        num_clients,
        api: API::TypicalHandover,
    };
    #[cfg(feature = "print-trace")]
    let download_time = start_timer!(|| "download for handover-dkg");
    let (download, mut stream) = download_from_network::<ClientDownloadHandover>(&server_ip, &request).await.unwrap();
    #[cfg(feature = "print-trace")]
    end_timer!(download_time);
    #[cfg(feature = "print-trace")]
    let upload_time = start_timer!(|| "upload for handover-dkg");
    upload_to_network::<ClientDownloadHandover>(&mut stream, &download).await.unwrap();
    #[cfg(feature = "print-trace")]
    end_timer!(upload_time);
}