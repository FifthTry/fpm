pub fn processor(
    section: &ftd::p1::Section,
    doc: &ftd::p2::TDoc,
    config: &fpm::Config,
) -> ftd::p1::Result<ftd::Value> {
    let name = match section.header.string(doc.name, section.line_number, "key") {
        Ok(name) => name,
        _ => {
            if let Some((_, name)) = section.name.rsplit_once(' ') {
                name.to_string()
            } else {
                section.name.to_string()
            }
        }
    };

    if section.body.is_some() && section.caption.is_some() {
        return Err(ftd::p1::Error::ParseError {
            message: "Cannot pass both caption and body".to_string(),
            doc_id: doc.name.to_string(),
            line_number: section.line_number,
        });
    }

    if let Some(data) = config.extra_data.get(name.as_str()) {
        return doc.from_json(data, section);
    }

    if let Some((_, ref b)) = section.body {
        return doc.from_json(&serde_json::from_str::<serde_json::Value>(b)?, section);
    }

    let caption = match section.caption {
        Some(ref caption) => caption,
        None => {
            return Err(ftd::p1::Error::ParseError {
                message: format!("Value is not passed for {}", name),
                doc_id: doc.name.to_string(),
                line_number: section.line_number,
            })
        }
    };

    if let Ok(val) = caption.parse::<bool>() {
        return doc.from_json(&serde_json::json!(val), section);
    }

    if let Ok(val) = caption.parse::<i64>() {
        return doc.from_json(&serde_json::json!(val), section);
    }

    if let Ok(val) = caption.parse::<f64>() {
        return doc.from_json(&serde_json::json!(val), section);
    }

    doc.from_json(&serde_json::json!(caption), section)
}
