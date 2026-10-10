//! Path parameter substitution for a resolved request URL.
//!
//! Only the path part of the URL is rewritten. A `:name` parameter must start a path segment,
//! so a port such as `:8080` is never read as a parameter. A `{name}` parameter may appear
//! anywhere in a segment, but never inside a `{{variable}}`. Values are percent-encoded, so a
//! value cannot add path segments, a query string or a variable.

use rocket_shared::types::PathParam;

/// Replaces `:name` and `{name}` path parameters in `url` with their percent-encoded values.
/// `params` values must already have their `{{variables}}` resolved. A parameter with an empty
/// name or an empty value is ignored, so its placeholder stays visible in the URL.
pub fn substitute_path_params(url: &str, params: &[PathParam]) -> String {
    let usable: Vec<(&str, String)> = params
        .iter()
        .filter(|p| !p.name.is_empty() && !p.value.is_empty())
        .map(|p| (p.name.as_str(), encode_path_param_value(&p.value)))
        .collect();
    if usable.is_empty() {
        return url.to_string();
    }
    let (start, end) = path_range(url);
    let rewritten = url[start..end]
        .split('/')
        .map(|segment| rewrite_segment(segment, &usable))
        .collect::<Vec<_>>()
        .join("/");
    format!("{}{}{}", &url[..start], rewritten, &url[end..])
}

/// The form a path parameter value takes in the URL. A caller that must find a substituted
/// value again, such as a later variable pass, uses it to match the URL text exactly.
pub fn encode_path_param_value(value: &str) -> String {
    urlencoding::encode(value).into_owned()
}

fn is_delimiter(c: char) -> bool {
    matches!(c, '/' | '?' | '#')
}

/// Byte range of the path inside `url`: after the scheme and authority, before `?` or `#`.
fn path_range(url: &str) -> (usize, usize) {
    let after_scheme = url
        .find("://")
        .filter(|i| !url[..*i].contains(is_delimiter))
        .map_or(0, |i| i + 3);
    let start = url[after_scheme..]
        .find(is_delimiter)
        .map_or(url.len(), |i| after_scheme + i);
    let end = url[start..]
        .find(['?', '#'])
        .map_or(url.len(), |i| start + i);
    (start, end)
}

fn rewrite_segment(segment: &str, params: &[(&str, String)]) -> String {
    let mut out = segment.to_string();
    if let Some(rest) = segment.strip_prefix(':') {
        let name_len = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        if let Some((_, encoded)) = params.iter().find(|(name, _)| *name == &rest[..name_len]) {
            out = format!("{encoded}{}", &rest[name_len..]);
        }
    }
    for (name, encoded) in params {
        out = replace_braced(&out, name, encoded);
    }
    out
}

/// Replaces `{name}` unless it is the inside of a `{{name}}` variable.
fn replace_braced(segment: &str, name: &str, encoded: &str) -> String {
    let needle = format!("{{{name}}}");
    let mut out = String::with_capacity(segment.len());
    let mut from = 0;
    while let Some(offset) = segment[from..].find(&needle) {
        let at = from + offset;
        let after = at + needle.len();
        out.push_str(&segment[from..at]);
        if segment[..at].ends_with('{') || segment[after..].starts_with('}') {
            out.push_str(&needle);
        } else {
            out.push_str(encoded);
        }
        from = after;
    }
    out.push_str(&segment[from..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str, value: &str) -> PathParam {
        PathParam {
            name: name.into(),
            value: value.into(),
            description: None,
        }
    }

    #[test]
    fn replaces_every_occurrence_of_a_colon_param() {
        let url = substitute_path_params("https://h.test/a/:id/b/:id", &[p("id", "7")]);
        assert_eq!(url, "https://h.test/a/7/b/7");
    }

    #[test]
    fn a_param_never_rewrites_a_longer_name() {
        let url = substitute_path_params("https://h.test/a/:idx/:id", &[p("id", "7")]);
        assert_eq!(url, "https://h.test/a/:idx/7");
    }

    #[test]
    fn keeps_a_name_followed_by_a_dot_suffix() {
        let url = substitute_path_params("https://h.test/files/:name.json", &[p("name", "r")]);
        assert_eq!(url, "https://h.test/files/r.json");
    }

    #[test]
    fn never_treats_a_port_as_a_param() {
        let url = substitute_path_params(
            "http://localhost:8080/u/:id",
            &[p("8080", "x"), p("id", "7")],
        );
        assert_eq!(url, "http://localhost:8080/u/7");
        let no_scheme =
            substitute_path_params("localhost:3000/u/:id", &[p("3000", "x"), p("id", "7")]);
        assert_eq!(no_scheme, "localhost:3000/u/7");
    }

    #[test]
    fn leaves_the_query_and_fragment_alone() {
        let url = substitute_path_params("https://h.test/a/:id?x=:id#:id", &[p("id", "7")]);
        assert_eq!(url, "https://h.test/a/7?x=:id#:id");
    }

    #[test]
    fn replaces_the_brace_form_but_never_inside_a_double_brace_variable() {
        let url = substitute_path_params("https://h.test/{id}/{{id}}", &[p("id", "7")]);
        assert_eq!(url, "https://h.test/7/{{id}}");
    }

    #[test]
    fn percent_encodes_the_value_so_it_cannot_add_segments_or_variables() {
        let url =
            substitute_path_params("https://h.test/a/:id", &[p("id", "x/../{{secret}} y?z#w")]);
        assert_eq!(
            url,
            "https://h.test/a/x%2F..%2F%7B%7Bsecret%7D%7D%20y%3Fz%23w"
        );
    }

    #[test]
    fn a_param_without_a_value_or_a_name_is_ignored() {
        let url = substitute_path_params("https://h.test/a/:id", &[p("id", ""), p("", "x")]);
        assert_eq!(url, "https://h.test/a/:id");
    }

    #[test]
    fn a_value_that_looks_like_another_param_is_not_substituted_again() {
        let url = substitute_path_params("https://h.test/:a/:b", &[p("a", ":b"), p("b", "2")]);
        assert_eq!(url, "https://h.test/%3Ab/2");
    }
}
