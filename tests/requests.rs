// Copyright 2023 Salesforce, Inc. All rights reserved.

mod common;

use anyhow::Ok;
use httpmock::MockServer;
use pdk_test::{pdk_test, TestComposite};
use pdk_test::port::Port;
use pdk_test::services::flex::{ApiConfig, Flex, FlexConfig, PolicyConfig};
use pdk_test::services::httpmock::{HttpMockConfig, HttpMock};
use std::time::Duration;

use common::*;

// Flex port for the internal test network
const FLEX_PORT: Port = 8081;

// This integration test shows how to build a test to compose a local-flex instance
// with a MockServer backend
#[pdk_test]
async fn block() -> anyhow::Result<()> {

    // Configure HttpMock service
    let http_mock_config = HttpMockConfig::builder()
        .hostname("backend")
        .port(80)
        .build();

    // Configure a Flex Service
    let policy_config = PolicyConfig::builder()
        .name(POLICY_NAME)
        .configuration(serde_json::json!({"source": "http://backend/blocked", "frequency": 60}))
        .build();

    // Configure the API to deploy to Flex
    let api_config = ApiConfig::builder()
        .name("ingress-http")
        .upstream(&http_mock_config)
        .path("/anything/echo")
        .port(FLEX_PORT)
        .policies([policy_config])
        .build();

    // Configure the Flex service
    let flex_config = FlexConfig::builder()
        .version("1.8.0")
        .hostname("local-flex")
        .config_mounts([
            (POLICY_DIR, "policy"),
            (COMMON_CONFIG_DIR, "common")
        ])
        .with_api(api_config)
        .build();

    // Register HttpMock service and start the docker network
    let composite = TestComposite::builder()
        .with_service(flex_config)
        .with_service(http_mock_config)
        .build()
        .await?;

    // Get a handle to the Flex Service
    let flex: Flex = composite.service()?;

    // Get an external URL to point the flex Service
    let flex_url = flex.external_url(FLEX_PORT).unwrap();
    
    // Get a handle to the HttpMock service
    let http_mock: HttpMock = composite.service()?;

    // Create a Mock Server
    let mock_server = MockServer::connect_async(http_mock.socket()).await;

    // Mock a '/api/status' request
    mock_server.mock_async(|when, then| {
        when.path_contains("/hello");
        then.status(200)
            .body("OK");
    }).await;

    let mock = mock_server.mock_async(|when, then| {
        when.path_contains("/blocked");
        then.status(200)
            .body("24.152.57.0/24\n24.232.0.0/16\n45.4.92.0/22");
    }).await;

    // wait 2 * freq for policy to fetch ips from seconds
    std::thread::sleep(Duration::from_secs(3));

    assert_request(&flex_url.as_str(), "24.152.58.1", 200).await?;
    //assert_request(&flex_url, "23.152.57.50", 200).await?;
    //assert_request(&flex_url, "24.152.57.1", 403).await?;
    //assert_request(&flex_url, "24.152.57.200", 403).await?;
    //assert_request(&flex_url, "24.233.0.1", 200).await?;
    //assert_request(&flex_url, "23.232.1.1", 200).await?;
    //assert_request(&flex_url, "24.232.10.50", 403).await?;
    //assert_request(&flex_url, "24.232.200.150", 403).await?;
    //assert_request(&flex_url, "45.4.96.1", 200).await?;
    //assert_request(&flex_url, "46.4.92.50", 200).await?;
    //assert_request(&flex_url, "45.4.93.10", 403).await?;
    //assert_request(&flex_url, "45.4.95.200", 403).await?;

    // Was only hit by one of the workers.
    mock.assert_hits(1);

    Ok(())
}

async fn assert_request(url: &str, ip: &str, status_code: u16) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{url}/hello"))
        .header("ip", ip)
        .send()
        .await?;

    assert_eq!(response.status().as_u16(), status_code);
    Ok(())
}
