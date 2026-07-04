use super::util::{split_and_trim, trim};
use crate::core_ir::Typ;

pub(crate) fn parse_in_type(s: &str) -> Typ {
    let s = trim(s);
    if s.eq_ignore_ascii_case("void") {
        return Typ::Void;
    }
    if s.starts_with('[') && s.ends_with(']') {
        return Typ::Array(Box::new(parse_in_type(&s[1..s.len() - 1])));
    }
    match s {
        "Int" => Typ::Int,
        "Float" => Typ::Float,
        "String" => Typ::String,
        "Bool" => Typ::Bool,
        "Void" => Typ::Void,
        other => Typ::Named(other.to_string()),
    }
}

pub(crate) fn parse_param(token: &str) -> (String, Typ) {
    match split_and_trim(':', token).as_slice() {
        [name, ty] => (trim(name).to_string(), parse_in_type(ty)),
        _ => (trim(token).to_string(), Typ::Named("Unknown".into())),
    }
}

pub(crate) fn parse_fn_header(after_fn_keyword: &str) -> (String, Vec<(String, Typ)>, Typ) {
    let after = trim(after_fn_keyword).trim_end_matches(';').trim();
    let open_idx = after.find('(');
    let close_idx = after.rfind(')');
    if let (Some(i), Some(j)) = (open_idx, close_idx)
        && j > i
    {
        let name = trim(&after[..i]).to_string();
        let param_blob = trim(&after[i + 1..j]);
        let params = if param_blob.is_empty() {
            Vec::new()
        } else {
            split_and_trim(',', param_blob)
                .into_iter()
                .map(|t| parse_param(&t))
                .collect()
        };
        let tail = after.get(j + 1..).unwrap_or("");
        let ret = match tail.split('>').collect::<Vec<_>>().as_slice() {
            [left, right] if trim(left).ends_with('-') => parse_in_type(right),
            _ => Typ::Void,
        };
        (name, params, ret)
    } else {
        (trim(after).to_string(), Vec::new(), Typ::Void)
    }
}
