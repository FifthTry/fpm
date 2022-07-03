pub async fn cr(
    title: Option<String>,
    description: Option<String>,
    cr_number: Option<usize>,
) -> fpm::Result<fpm::apis::cr::CreateResponse> {
    #[derive(Debug, serde::Deserialize)]
    struct ApiResponse {
        message: Option<String>,
        data: Option<fpm::apis::cr::CreateResponse>,
        success: bool,
    }

    let url = format!("{}/-/cr/", fpm::commands::utils::remote_host());
    let cr_request = fpm::apis::cr::CreateRequest {
        title,
        description,
        cr_number,
    };

    let data = serde_json::to_string(&cr_request)?;

    let mut response = reqwest::Client::new()
        .post(url.as_str())
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(data)
        .send()?;

    let response = response.json::<ApiResponse>()?;
    if !response.success {
        return Err(fpm::Error::APIResponseError(
            response
                .message
                .unwrap_or_else(|| "Some Error occurred".to_string()),
        ));
    }

    match response.data {
        Some(data) => Ok(data),
        None => Err(fpm::Error::APIResponseError(
            "Unexpected API behaviour".to_string(),
        )),
    }
}
