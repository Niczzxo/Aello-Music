//! Pure-Rust signature deciphering (no JS engine, no WebView).
//!
//! Android runs the player JS in a WebView (`CipherDeobfuscator` +
//! `CipherWebView`). This port does the same job the way yt-dlp does:
//! parse the decipher function out of `base.js`, classify the helper
//! object's methods (reverse / splice / slice / swap), and apply the
//! recorded operations to the signature string.
//!
//! If the player JS shape changes so the function cannot be extracted,
//! callers get [`StreamError::CipherUnsupported`] — never a broken URL.

use crate::innertube::error::StreamError;
use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpKind {
    Reverse,
    Splice,
    Slice,
    Swap,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Op {
    kind: OpKind,
    /// Numeric argument from the call site (`XX.fn(a, N)`); 0 when absent.
    arg: usize,
}

/// Find the index of the `}` matching the `{` at `open` (byte offsets).
fn matching_brace(src: &str, open: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    let mut in_str: Option<u8> = None;
    let mut escaped = false;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = in_str {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == q {
                in_str = None;
            }
        } else {
            match b {
                b'"' | b'\'' | b'`' => in_str = Some(b),
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    None
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("static regex")
}

/// Classify one helper-object method body into an operation kind.
fn classify_method_body(body: &str) -> Option<OpKind> {
    if body.contains(".reverse()") {
        Some(OpKind::Reverse)
    } else if body.contains(".splice(0,") || body.contains(".splice(0 ,") {
        Some(OpKind::Splice)
    } else if body.contains(".slice(") {
        Some(OpKind::Slice)
    } else if body.contains("[0]=") && (body.contains("%") || body.contains("length")) {
        // swap: `var c=a[0];a[0]=a[b%a.length];a[b%a.length]=c`
        Some(OpKind::Swap)
    } else {
        None
    }
}

/// Extract the decipher plan (ordered operations) from player JS source.
///
/// Returns `None` when the JS shape is not recognised — the caller turns
/// that into [`StreamError::CipherUnsupported`].
pub fn extract_decipher_plan(js: &str) -> Option<Vec<Op>> {
    // 1. Locate the decipher function: it splits its argument into chars.
    //    `XX=function(a){a=a.split("")...}`, `function XX(a){...}`, or
    //    the arrow forms yt-dlp also handles: `XX=(a)=>{...}` /
    //    `XX=a=>{...}`. Search for the split("") anchor and walk back
    //    to the enclosing function head.
    let split_re = re(r#"(\w)=(\w)\.split\(""\)"#);
    let caps = split_re.captures(js)?;
    let param = caps.get(2)?.as_str().to_string();
    let split_pos = caps.get(0)?.start();

    // Function head: find the nearest `=function(P){`, `function NAME(P){`,
    // or arrow `NAME=(P)=>{` / `NAME=P=>{` before split_pos.
    let head_re = re(&format!(
        r"(?:([A-Za-z_$][\w$]*)\s*=\s*function\s*\({p}\)|function ([A-Za-z_$][\w$]*)\s*\({p}\)|([A-Za-z_$][\w$]*)\s*=\s*\(?{p}\)?\s*=>)\s*\{{",
        p = regex::escape(&param)
    ));
    let mut fn_name: Option<String> = None;
    let mut fn_body_start: Option<usize> = None;
    for c in head_re.captures_iter(&js[..split_pos]) {
        fn_name = Some(
            c.get(1)
                .or_else(|| c.get(2))
                .or_else(|| c.get(3))
                .map(|m| m.as_str().to_string())?,
        );
        fn_body_start = Some(c.get(0)?.end() - 1); // position of `{`
    }
    let body_start = fn_body_start?;
    let body_end = matching_brace(js, body_start)?;
    let body = &js[body_start + 1..body_end];

    // 2. Find the helper object name: first `OBJ.method(param, ...)` call.
    let call_re = re(&format!(
        r"([A-Za-z_$][\w$]*)\.[A-Za-z_$][\w$]*\({p}\b",
        p = regex::escape(&param)
    ));
    let obj_name = call_re
        .captures(body)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string());

    // 3. Parse the helper object definition `var OBJ={...}` and classify
    //    each method.
    let mut methods: Vec<(String, OpKind)> = Vec::new();
    if let Some(obj) = obj_name {
        let obj_re = re(&format!(
            r"var {}\s*=\s*\{{",
            regex::escape(&obj)
        ));
        if let Some(m) = obj_re.find(js) {
            let open = m.end() - 1;
            if let Some(close) = matching_brace(js, open) {
                let obj_body = &js[open + 1..close];
                // Methods: `name:function(a,b){...}`, `name:(a,b)=>{...}`,
                // or shorthand `name(a,b){...}`.
                let method_re = re(
                    r"(?s)([A-Za-z_$][\w$]*)\s*:\s*(?:function\s*\([^)]*\)|\(?[^)]*\)?\s*=>)\s*\{",
                );
                for mc in method_re.captures_iter(obj_body) {
                    let name = mc.get(1)?.as_str().to_string();
                    let brace = mc.get(0)?.end() - 1;
                    // brace is relative to obj_body start
                    let abs_open = open + 1 + brace;
                    if let Some(abs_close) = matching_brace(js, abs_open) {
                        let mbody = &js[abs_open + 1..abs_close];
                        if let Some(kind) = classify_method_body(mbody) {
                            methods.push((name, kind));
                        }
                    }
                }
                // Shorthand methods `name(a,b){...}` not caught above.
                if methods.is_empty() {
                    let short_re =
                        re(r"(?s)([A-Za-z_$][\w$]*)\s*\([^)]*\)\s*\{");
                    for mc in short_re.captures_iter(obj_body) {
                        let name = mc.get(1)?.as_str().to_string();
                        let brace = mc.get(0)?.end() - 1;
                        let abs_open = open + 1 + brace;
                        if let Some(abs_close) = matching_brace(js, abs_open) {
                            let mbody = &js[abs_open + 1..abs_close];
                            if let Some(kind) = classify_method_body(mbody) {
                                methods.push((name, kind));
                            }
                        }
                    }
                }
            }
        }
        // 4. Read the operations from the decipher body, in order.
        let mut ops: Vec<Op> = Vec::new();
        // `OBJ.method(a)` / `OBJ.method(a, N)` / `a.reverse()` /
        // `a=a.slice(N)` / `a.splice(0,N)` inline forms.
        let op_re = re(&format!(
            r"(?:{obj}\.([A-Za-z_$][\w$]*)\({p}(?:,(\d+))?\)|{p}\.reverse\(\)|{p}={p}\.slice\((\d+)\)|{p}\.splice\(0,(\d+)\))",
            obj = regex::escape(&obj),
            p = regex::escape(&param),
        ));
        for oc in op_re.captures_iter(body) {
            if let Some(m) = oc.get(1) {
                let kind = methods
                    .iter()
                    .find(|(n, _)| n == m.as_str())
                    .map(|(_, k)| *k);
                if let Some(kind) = kind {
                    let arg = oc
                        .get(2)
                        .and_then(|a| a.as_str().parse().ok())
                        .unwrap_or(0);
                    ops.push(Op { kind, arg });
                }
            } else if oc.get(0)?.as_str().contains(".reverse()") {
                ops.push(Op { kind: OpKind::Reverse, arg: 0 });
            } else if let Some(n) = oc.get(3).or_else(|| oc.get(4)) {
                let arg: usize = n.as_str().parse().unwrap_or(0);
                let kind = if oc.get(3).is_some() {
                    OpKind::Slice
                } else {
                    OpKind::Splice
                };
                ops.push(Op { kind, arg });
            }
        }
        let _ = fn_name;
        if ops.is_empty() {
            return None;
        }
        return Some(ops);
    }

    // No helper object: the body may only use inline ops (reverse/slice).
    let mut ops: Vec<Op> = Vec::new();
    let inline_re = re(&format!(
        r"(?:{p}\.reverse\(\)|{p}={p}\.slice\((\d+)\)|{p}\.splice\(0,(\d+)\))",
        p = regex::escape(&param)
    ));
    for oc in inline_re.captures_iter(body) {
        if oc.get(0)?.as_str().contains(".reverse()") {
            ops.push(Op { kind: OpKind::Reverse, arg: 0 });
        } else if let Some(n) = oc.get(1).or_else(|| oc.get(2)) {
            let arg: usize = n.as_str().parse().unwrap_or(0);
            let kind = if oc.get(1).is_some() {
                OpKind::Slice
            } else {
                OpKind::Splice
            };
            ops.push(Op { kind, arg });
        }
    }
    if ops.is_empty() {
        None
    } else {
        Some(ops)
    }
}

fn apply_ops(ops: &[Op], sig: &str) -> String {
    let mut chars: Vec<char> = sig.chars().collect();
    for op in ops {
        match op.kind {
            OpKind::Reverse => chars.reverse(),
            OpKind::Splice | OpKind::Slice => {
                let n = op.arg.min(chars.len());
                chars.drain(..n);
            }
            OpKind::Swap => {
                if chars.len() > 1 {
                    let j = op.arg % chars.len();
                    chars.swap(0, j);
                }
            }
        }
    }
    chars.into_iter().collect()
}

/// Decipher one obfuscated signature using the given player JS source.
pub fn decipher_signature(js: &str, signature: &str) -> Result<String, StreamError> {
    let ops = extract_decipher_plan(js).ok_or_else(|| {
        StreamError::CipherUnsupported(
            "player JS decipher function not recognised (shape changed)".to_string(),
        )
    })?;
    Ok(apply_ops(&ops, signature))
}

/// Parse a `signatureCipher` / `cipher` query string (`s=…&sp=…&url=…`)
/// and return `(base_url, obfuscated_sig, sig_param)`.
pub fn parse_cipher(cipher: &str) -> Option<(String, String, String)> {
    let mut url: Option<String> = None;
    let mut sig: Option<String> = None;
    let mut sp = "signature".to_string();
    for (k, v) in url::form_urlencoded::parse(cipher.as_bytes()) {
        match k.as_ref() {
            "url" => url = Some(v.into_owned()),
            "s" => sig = Some(v.into_owned()),
            "sp" => sp = v.into_owned(),
            _ => {}
        }
    }
    Some((url?, sig?, sp))
}

/// Join a deciphered signature onto its base URL (`sp=sig`, URL-encoded).
pub fn join_signature(base_url: &str, sig_param: &str, sig: &str) -> String {
    let sep = if base_url.contains('?') { "&" } else { "?" };
    let encoded: String = url::form_urlencoded::byte_serialize(sig.as_bytes()).collect();
    format!("{base_url}{sep}{sig_param}={encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic base.js: helper object with one method per op kind
    /// (reverse / splice / swap), plus a decipher function in the
    /// given head form. The plan is always
    /// [Reverse, Splice(2), Swap(3)], so `decipher_signature` maps
    /// "abcdef" → reverse "fedcba" → splice 2 "dcba" → swap(0,3) "acbd".
    fn js_with_decipher(head: &str) -> String {
        format!(
            "var AB={{a:function(x){{x.reverse()}},b:function(x,y){{x.splice(0,y)}},c:function(x,y){{var z=x[0];x[0]=x[y%x.length];x[y%x.length]=z}}}};{head}"
        )
    }

    fn expected_ops() -> Vec<(OpKind, usize)> {
        vec![(OpKind::Reverse, 0), (OpKind::Splice, 2), (OpKind::Swap, 3)]
    }

    fn assert_plan(js: &str) {
        let ops = extract_decipher_plan(js).expect("plan should extract");
        let got: Vec<(OpKind, usize)> = ops.iter().map(|op| (op.kind, op.arg)).collect();
        assert_eq!(got, expected_ops());
        assert_eq!(decipher_signature(js, "abcdef").unwrap(), "acbd");
    }

    #[test]
    fn decipher_function_form() {
        let js = js_with_decipher(
            r#"var CD=function(a){a=a.split("");AB.a(a);AB.b(a,2);AB.c(a,3);return a.join("")};"#,
        );
        assert_plan(&js);
    }

    #[test]
    fn decipher_function_declaration_form() {
        let js = js_with_decipher(
            r#"function CD(a){a=a.split("");AB.a(a);AB.b(a,2);AB.c(a,3);return a.join("")}"#,
        );
        assert_plan(&js);
    }

    #[test]
    fn decipher_arrow_paren_form() {
        let js = js_with_decipher(
            r#"var CD=(a)=>{a=a.split("");AB.a(a);AB.b(a,2);AB.c(a,3);return a.join("")};"#,
        );
        assert_plan(&js);
    }

    #[test]
    fn decipher_arrow_bare_param_form() {
        let js = js_with_decipher(
            r#"var CD=a=>{a=a.split("");AB.a(a);AB.b(a,2);AB.c(a,3);return a.join("")};"#,
        );
        assert_plan(&js);
    }

    #[test]
    fn decipher_unrecognised_shape_returns_none() {
        assert!(extract_decipher_plan("var x = 1;").is_none());
    }
}
