use std::net::IpAddr;

use crate::cli::Cache;

#[derive(Debug, Clone)]
pub struct ApplicationSettings {
    pub port: u16,
    pub host: IpAddr,
    pub cache: Cache,
}

impl ApplicationSettings {
    #[must_use]
    pub fn new(port: u16, host: IpAddr, cache: Cache) -> Self {
        Self { port, host, cache }
    }
}

#[derive(Debug, Clone)]
pub struct Template {
    pub template: String,
    pub fields: Vec<String>,
}

impl TryFrom<String> for Template {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() {
            return Err(anyhow::anyhow!("Un template no puede ser un string vacío",));
        }

        let mut start = 0;
        let separator = "{{";
        let separator_len = separator.len();
        let mut fields = Vec::new();
        let mut sql_template = String::new();

        while let Some(open_idx) = value[start..].find("{{") {
            if let Some(close_idx) = value[start + open_idx..].find("}}") {
                let field = &value[start + open_idx + separator_len..start + open_idx + close_idx];
                fields.push(field.trim().to_string());

                let label = &value[start..start + open_idx].trim();

                if !sql_template.is_empty() {
                    sql_template.push(' ');
                }
                sql_template.push_str(&format!("' {} ' || {} ||", label, field.trim()));

                start += open_idx + close_idx + separator_len;
            } else {
                return Err(anyhow::anyhow!("El template está mal conformado"));
            }
        }

        if sql_template.ends_with("||") {
            sql_template.truncate(sql_template.len() - 3);
        }

        if start < value.len() {
            let remaining_text = &value[start..].trim();
            if !remaining_text.is_empty() {
                if !sql_template.is_empty() {
                    sql_template.push(' ');
                }
                sql_template.push_str(remaining_text);
            }
        }

        Ok(Self {
            template: sql_template,
            fields,
        })
    }
}
