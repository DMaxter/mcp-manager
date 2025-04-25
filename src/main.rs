use std::env;

use mcp_manager::{config::get_config, models::AIModel};
use tokio::io::{self, AsyncBufReadExt, BufReader, stdin};

const CONFIG_FILE: &str = "config.yaml";

#[tokio::main]
async fn main() -> io::Result<()> {
    let config_file = env::var_os("MCP_MANAGER_CONFIG").map_or(CONFIG_FILE.to_owned(), |var| {
        var.into_string().unwrap_or(CONFIG_FILE.to_owned())
    });

    let config = get_config(&config_file)?;

    let mut reader = BufReader::new(stdin());
    let mut line = String::new();
    let mut prompt = String::new();

    println!("Send you prompt. When you are finished, press Ctrl+D");

    loop {
        line.clear();

        let bytes_read = reader.read_line(&mut line).await?;

        if bytes_read == 0 {
            break;
        }

        prompt.push_str(&line);
    }

    Ok(())
}
