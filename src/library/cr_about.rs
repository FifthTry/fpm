pub async fn processor<'a>(
    section: &ftd::p1::Section,
    doc: &ftd::p2::TDoc<'a>,
    config: &fpm::Config,
) -> ftd::p1::Result<ftd::Value> {
    processor_(section, doc, config)
        .await
        .map_err(|e| ftd::p1::Error::ParseError {
            message: e.to_string(),
            doc_id: doc.name.to_string(),
            line_number: section.line_number,
        })
}

pub async fn processor_<'a>(
    section: &ftd::p1::Section,
    doc: &ftd::p2::TDoc<'a>,
    config: &fpm::Config,
) -> fpm::Result<ftd::Value> {
    if let Some(ref id) = config.current_document {
        if let Some((cr_number, _)) = fpm::cr::get_cr_and_path_from_id(id, &None) {
            let cr_about = fpm::cr::get_cr_about(config, cr_number).await?;
            return Ok(doc.from_json(&cr_about, section)?);
        }
    }
    fpm::usage_error("NO CR found, error in resolving `cr-about` processor".to_string())
}
