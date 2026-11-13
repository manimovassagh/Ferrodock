//! Reading the small XML documents S3 requests carry (which objects to
//! delete, which parts make an upload, a bucket's tags) and writing the
//! pieces replies share. Requests are searched by element name: enough
//! for documents this regular, with no parser to bring in.

use crate::protocol::xml_escape;

pub const NAMESPACE: &str = "http://s3.amazonaws.com/doc/2006-03-01/";

/// A reply's document: the XML declaration, then `root` holding `inner`.
pub fn document(root: &str, inner: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<{root} xmlns=\"{NAMESPACE}\">{inner}</{root}>"
    )
}

/// `<name>text</name>`, escaped.
pub fn element(name: &str, text: &str) -> String {
    format!("<{name}>{}</{name}>", xml_escape(text))
}

/// Where the element `name` opens in `xml`: the index after its opening
/// tag, skipping tags that merely start with the name.
fn open(xml: &str, name: &str) -> Option<(usize, usize)> {
    let mut from = 0;
    loop {
        let at = from + xml[from..].find(&format!("<{name}"))?;
        let after = at + 1 + name.len();
        match xml[after..].chars().next()? {
            '>' => return Some((at, after + 1)),
            // Attributes, or a namespace.
            c if c.is_whitespace() => {
                let close = after + xml[after..].find('>')?;
                if xml[..close].ends_with('/') {
                    from = close;
                    continue;
                }
                return Some((at, close + 1));
            }
            _ => from = after,
        }
    }
}

/// What's inside each element called `name`, in order. Elements of that
/// name aren't expected inside one another.
pub fn blocks<'a>(xml: &'a str, name: &str) -> Vec<&'a str> {
    let close = format!("</{name}>");
    let mut found = Vec::new();
    let mut rest = xml;
    while let Some((_, start)) = open(rest, name) {
        let Some(end) = rest[start..].find(&close) else {
            break;
        };
        found.push(&rest[start..start + end]);
        rest = &rest[start + end + close.len()..];
    }
    found
}

/// The text of the first element called `name`, unescaped.
pub fn text(xml: &str, name: &str) -> Option<String> {
    blocks(xml, name)
        .first()
        .map(|inner| unescape(inner.trim()))
}

/// Undoes [`xml_escape`], and numeric references.
pub fn unescape(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';').filter(|end| *end <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let known = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|n| n.parse().ok()))
                .and_then(char::from_u32),
        };
        match known {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out + rest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_requests_carry() {
        let delete = "<?xml version=\"1.0\"?><Delete xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
                      <Object><Key>a &amp; b.txt</Key></Object>\
                      <Object><Key>c.txt</Key><VersionId>v1</VersionId></Object><Quiet>true</Quiet></Delete>";
        let objects = blocks(delete, "Object");
        assert_eq!(objects.len(), 2);
        assert_eq!(text(objects[0], "Key").as_deref(), Some("a & b.txt"));
        assert_eq!(text(objects[0], "VersionId"), None);
        assert_eq!(text(objects[1], "VersionId").as_deref(), Some("v1"));
        assert_eq!(text(delete, "Quiet").as_deref(), Some("true"));
    }

    #[test]
    fn tells_elements_from_longer_names() {
        // `Tag` inside `TagSet`, and `Part` next to `PartNumber`.
        let tagging = "<Tagging><TagSet><Tag><Key>env</Key><Value>dev</Value></Tag>\
                       <Tag><Key>team</Key><Value>a&#38;b</Value></Tag></TagSet></Tagging>";
        let tags = blocks(tagging, "Tag");
        assert_eq!(tags.len(), 2);
        assert_eq!(text(tags[1], "Value").as_deref(), Some("a&b"));
        let parts = "<CompleteMultipartUpload><Part><PartNumber>1</PartNumber><ETag>&quot;abc&quot;</ETag></Part>\
                     </CompleteMultipartUpload>";
        assert_eq!(blocks(parts, "Part").len(), 1);
        assert_eq!(text(parts, "PartNumber").as_deref(), Some("1"));
        assert_eq!(text(parts, "ETag").as_deref(), Some("\"abc\""));
        assert_eq!(
            text("<Status xmlns=\"x\">Enabled</Status>", "Status").as_deref(),
            Some("Enabled")
        );
        assert_eq!(text("<A/><B>1</B>", "A"), None);
    }

    #[test]
    fn writes_documents() {
        assert_eq!(
            document("ListBucketResult", &element("Name", "a<b")),
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <ListBucketResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\"><Name>a&lt;b</Name></ListBucketResult>"
        );
        assert_eq!(unescape("a &amp; b &unknown; &"), "a & b &unknown; &");
    }
}
