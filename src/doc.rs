//! Built-in documentation generator for Nacre (`nacre doc`).
//! Generates a modern, zero-dependency, searchable static HTML documentation site from `///` doc comments.

use std::fs;
use std::path::{Path, PathBuf};
use crate::ast::{Function, Program, Type, Visibility};
use crate::project::SourceBundle;

pub fn generate_docs(sources: &[PathBuf], output_dir: Option<&Path>) -> Result<(), String> {
    let out = output_dir.unwrap_or_else(|| Path::new("doc"));
    fs::create_dir_all(out).map_err(|e| format!("failed to create output directory {}: {e}", out.display()))?;

    let bundle = SourceBundle::from_paths(sources)?;
    let parsed = bundle.parse().map_err(|d| bundle.render(&d))?;

    let html = render_documentation(&parsed, &bundle);
    let index_path = out.join("index.html");
    fs::write(&index_path, html).map_err(|e| format!("failed to write {}: {e}", index_path.display()))?;

    println!("Generated documentation in {} (index: {})", out.display(), index_path.display());
    Ok(())
}

pub fn extract_doc_comment(source: &str, span_start: usize) -> Option<String> {
    let before = &source[..span_start.min(source.len())];
    let mut lines = Vec::new();
    let mut current = before;

    while let Some(last_nl) = current.rfind('\n') {
        let line = current[last_nl + 1..].trim();
        if let Some(doc) = line.strip_prefix("///") {
            lines.push(doc.trim().to_string());
            current = &current[..last_nl];
        } else if line.is_empty() {
            if lines.is_empty() {
                current = &current[..last_nl];
            } else {
                break;
            }
        } else {
            break;
        }
    }

    let first_line = current.trim();
    if let Some(doc) = first_line.strip_prefix("///") {
        lines.push(doc.trim().to_string());
    }

    if lines.is_empty() {
        None
    } else {
        lines.reverse();
        Some(lines.join("\n"))
    }
}

pub fn render_documentation(program: &Program, bundle: &SourceBundle) -> String {
    // Group items: Classes, Interfaces, Enums, Free Functions
    let mut classes = Vec::new();
    let mut interfaces = Vec::new();
    let mut enums = Vec::new();

    for class in &program.classes {
        if class.is_builtin {
            continue;
        }
        let loc = bundle.locate(class.span);
        let doc = extract_doc_comment(loc.source, loc.span.start);

        if class.is_interface {
            interfaces.push((class, doc));
        } else if class.variants.is_some() {
            enums.push((class, doc));
        } else {
            classes.push((class, doc));
        }
    }

    let mut functions = Vec::new();
    for function in &program.functions {
        let loc = bundle.locate(function.span);
        let doc = extract_doc_comment(loc.source, loc.span.start);
        functions.push((function, doc));
    }

    let mut html = String::new();
    html.push_str(r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Nacre API Documentation</title>
<style>
:root {
  --bg: #121417;
  --surface: #1a1e24;
  --surface-border: #2e3642;
  --text: #e6edf3;
  --text-muted: #8b949e;
  --accent: #58a6ff;
  --accent-badge: #1f6feb;
  --keyword: #ff7b72;
  --type: #79c0ff;
  --code-bg: #0d1117;
}
* { box-sizing: border-box; margin: 0; padding: 0; }
body {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
  background-color: var(--bg);
  color: var(--text);
  display: flex;
  min-height: 100vh;
}
sidebar {
  width: 280px;
  background: var(--surface);
  border-right: 1px solid var(--surface-border);
  padding: 24px 16px;
  position: sticky;
  top: 0;
  height: 100vh;
  overflow-y: auto;
  flex-shrink: 0;
}
sidebar h2 { font-size: 1.2rem; color: var(--accent); margin-bottom: 16px; }
sidebar input {
  width: 100%;
  padding: 8px 12px;
  background: var(--code-bg);
  border: 1px solid var(--surface-border);
  border-radius: 6px;
  color: var(--text);
  margin-bottom: 16px;
}
sidebar nav section { margin-bottom: 16px; }
sidebar nav h3 { font-size: 0.85rem; text-transform: uppercase; color: var(--text-muted); margin-bottom: 8px; }
sidebar nav a { display: block; color: var(--text); text-decoration: none; padding: 4px 8px; border-radius: 4px; font-size: 0.9rem; }
sidebar nav a:hover { background: var(--surface-border); color: var(--accent); }
main {
  flex-grow: 1;
  padding: 40px 60px;
  max-width: 1100px;
}
header h1 { font-size: 2.2rem; margin-bottom: 8px; color: var(--text); }
header p { color: var(--text-muted); font-size: 1.1rem; margin-bottom: 32px; }
.card {
  background: var(--surface);
  border: 1px solid var(--surface-border);
  border-radius: 8px;
  padding: 24px;
  margin-bottom: 24px;
}
.card h3 {
  font-size: 1.4rem;
  margin-bottom: 12px;
  display: flex;
  align-items: center;
  gap: 10px;
}
.badge {
  font-size: 0.75rem;
  text-transform: uppercase;
  padding: 2px 8px;
  border-radius: 12px;
  background: var(--accent-badge);
  color: white;
}
.badge.interface { background: #8957e5; }
.badge.enum { background: #d29922; }
.badge.fn { background: #238636; }
.docstring {
  color: #c9d1d9;
  line-height: 1.6;
  margin: 12px 0;
}
.docstring pre {
  background: var(--code-bg);
  padding: 12px;
  border-radius: 6px;
  margin: 8px 0;
  overflow-x: auto;
}
.signature {
  font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
  background: var(--code-bg);
  padding: 10px 14px;
  border-radius: 6px;
  border-left: 3px solid var(--accent);
  margin-bottom: 16px;
  font-size: 0.95rem;
}
table { width: 100%; border-collapse: collapse; margin-top: 12px; }
th, td { text-align: left; padding: 8px 12px; border-bottom: 1px solid var(--surface-border); }
th { color: var(--text-muted); font-size: 0.85rem; text-transform: uppercase; }
code { font-family: monospace; background: var(--code-bg); padding: 2px 6px; border-radius: 4px; font-size: 0.9em; }
</style>
<script>
function filterItems() {
  const query = document.getElementById('search').value.toLowerCase();
  document.querySelectorAll('.card').forEach(card => {
    const text = card.textContent.toLowerCase();
    card.style.display = text.includes(query) ? 'block' : 'none';
  });
}
</script>
</head>
<body>
<sidebar>
  <h2>Nacre Docs</h2>
  <input type="text" id="search" placeholder="Search API..." oninput="filterItems()">
  <nav>
"#);

    if !classes.is_empty() {
        html.push_str("<section><h3>Classes</h3>");
        for (c, _) in &classes {
            html.push_str(&format!("<a href=\"#{}\">{}</a>", c.name, c.name));
        }
        html.push_str("</section>");
    }

    if !interfaces.is_empty() {
        html.push_str("<section><h3>Interfaces</h3>");
        for (i, _) in &interfaces {
            html.push_str(&format!("<a href=\"#{}\">{}</a>", i.name, i.name));
        }
        html.push_str("</section>");
    }

    if !enums.is_empty() {
        html.push_str("<section><h3>Enums</h3>");
        for (e, _) in &enums {
            html.push_str(&format!("<a href=\"#{}\">{}</a>", e.name, e.name));
        }
        html.push_str("</section>");
    }

    if !functions.is_empty() {
        html.push_str("<section><h3>Functions</h3>");
        for (f, _) in &functions {
            html.push_str(&format!("<a href=\"#{}\">{}</a>", f.name, f.name));
        }
        html.push_str("</section>");
    }

    html.push_str(r#"
  </nav>
</sidebar>
<main>
  <header>
    <h1>Project Documentation</h1>
    <p>API documentation generated automatically by Nacre compiler.</p>
  </header>
"#);

    // Classes section
    for (class, doc) in &classes {
        html.push_str(&format!("<div class=\"card\" id=\"{}\">", class.name));
        html.push_str(&format!("<h3><span class=\"badge\">class</span> {}</h3>", class.name));

        let mut sig = format!("class {}", class.name);
        if !class.type_params.is_empty() {
            sig.push_str(&format!("<{}>", class.type_params.join(", ")));
        }
        if !class.implements.is_empty() {
            sig.push_str(" implements ");
            let impls: Vec<_> = class.implements.iter().map(format_type).collect();
            sig.push_str(&impls.join(", "));
        }
        html.push_str(&format!("<div class=\"signature\">{}</div>", escape_html(&sig)));

        if let Some(doc_str) = doc {
            html.push_str(&format!("<div class=\"docstring\">{}</div>", render_markdown(doc_str)));
        }

        if !class.fields.is_empty() {
            html.push_str("<h4>Fields</h4><table><tr><th>Visibility</th><th>Type</th><th>Name</th></tr>");
            for f in &class.fields {
                let vis = match f.visibility { Visibility::Public => "public", Visibility::Private => "private" };
                html.push_str(&format!(
                    "<tr><td><code>{}</code></td><td><code>{}</code></td><td>{}</td></tr>",
                    vis, escape_html(&format_type(&f.ty)), f.name
                ));
            }
            html.push_str("</table><br>");
        }

        if !class.methods.is_empty() {
            html.push_str("<h4>Methods</h4><table><tr><th>Signature</th><th>Description</th></tr>");
            for m in &class.methods {
                let loc = bundle.locate(m.span);
                let m_doc = extract_doc_comment(loc.source, loc.span.start);
                let desc = m_doc.as_deref().unwrap_or("");
                html.push_str(&format!(
                    "<tr><td><code>{}</code></td><td>{}</td></tr>",
                    escape_html(&format_function_signature(m)),
                    escape_html(desc)
                ));
            }
            html.push_str("</table>");
        }

        html.push_str("</div>");
    }

    // Interfaces section
    for (iface, doc) in &interfaces {
        html.push_str(&format!("<div class=\"card\" id=\"{}\">", iface.name));
        html.push_str(&format!("<h3><span class=\"badge interface\">interface</span> {}</h3>", iface.name));
        html.push_str(&format!("<div class=\"signature\">interface {}</div>", iface.name));

        if let Some(doc_str) = doc {
            html.push_str(&format!("<div class=\"docstring\">{}</div>", render_markdown(doc_str)));
        }

        if !iface.methods.is_empty() {
            html.push_str("<h4>Methods</h4><table><tr><th>Signature</th><th>Description</th></tr>");
            for m in &iface.methods {
                let loc = bundle.locate(m.span);
                let m_doc = extract_doc_comment(loc.source, loc.span.start);
                let desc = m_doc.as_deref().unwrap_or("");
                html.push_str(&format!(
                    "<tr><td><code>{}</code></td><td>{}</td></tr>",
                    escape_html(&format_function_signature(m)),
                    escape_html(desc)
                ));
            }
            html.push_str("</table>");
        }

        html.push_str("</div>");
    }

    // Enums section
    for (en, doc) in &enums {
        html.push_str(&format!("<div class=\"card\" id=\"{}\">", en.name));
        html.push_str(&format!("<h3><span class=\"badge enum\">enum</span> {}</h3>", en.name));
        html.push_str(&format!("<div class=\"signature\">enum {}</div>", en.name));

        if let Some(doc_str) = doc {
            html.push_str(&format!("<div class=\"docstring\">{}</div>", render_markdown(doc_str)));
        }

        if let Some(variants) = &en.variants {
            html.push_str("<h4>Variants</h4><table><tr><th>Variant</th><th>Payload</th></tr>");
            for v in variants {
                let fields: Vec<_> = v.fields.iter().map(format_type).collect();
                let payload = if fields.is_empty() { "none".to_string() } else { fields.join(", ") };
                html.push_str(&format!("<tr><td><code>{}</code></td><td><code>{}</code></td></tr>", v.name, escape_html(&payload)));
            }
            html.push_str("</table>");
        }

        html.push_str("</div>");
    }

    // Free functions
    for (func, doc) in &functions {
        html.push_str(&format!("<div class=\"card\" id=\"{}\">", func.name));
        html.push_str(&format!("<h3><span class=\"badge fn\">fn</span> {}</h3>", func.name));
        html.push_str(&format!("<div class=\"signature\">{}</div>", escape_html(&format_function_signature(func))));

        if let Some(doc_str) = doc {
            html.push_str(&format!("<div class=\"docstring\">{}</div>", render_markdown(doc_str)));
        }
        html.push_str("</div>");
    }

    html.push_str("</main></body></html>\n");
    html
}

fn format_function_signature(f: &Function) -> String {
    let mut s = String::new();
    match f.visibility {
        Visibility::Public => s.push_str("public "),
        Visibility::Private => s.push_str("private "),
    }
    if f.is_static {
        s.push_str("static ");
    }
    if f.readonly {
        s.push_str("readonly ");
    }
    s.push_str(&format_type(&f.return_type));
    s.push(' ');
    s.push_str(&f.name);
    s.push('(');
    let params: Vec<_> = f.params.iter().map(|p| format!("{} {}", format_type(&p.ty), p.name)).collect();
    s.push_str(&params.join(", "));
    s.push(')');
    s
}

fn format_type(ty: &Type) -> String {
    match ty {
        Type::Unit => "Unit".to_owned(),
        Type::Byte => "byte".to_owned(),
        Type::Char => "char".to_owned(),
        Type::Int => "int".to_owned(),
        Type::Long => "long".to_owned(),
        Type::Double => "double".to_owned(),
        Type::Boolean => "boolean".to_owned(),
        Type::String => "String".to_owned(),
        Type::Void => "void".to_owned(),
        Type::Named(name) => name.clone(),
        Type::Applied { name, args } => {
            let inner: Vec<_> = args.iter().map(format_type).collect();
            format!("{name}<{}>", inner.join(", "))
        }
        Type::Borrow { name, args, mutable } => {
            let mut s = if *mutable { "borrow mut ".to_string() } else { "borrow ".to_string() };
            s.push_str(name);
            if !args.is_empty() {
                let inner: Vec<_> = args.iter().map(format_type).collect();
                s.push_str(&format!("<{}>", inner.join(", ")));
            }
            s
        }
    }
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn render_markdown(s: &str) -> String {
    let mut out = String::new();
    let mut in_code_block = false;

    for line in s.lines() {
        if line.starts_with("```") {
            if in_code_block {
                out.push_str("</code></pre>\n");
                in_code_block = false;
            } else {
                out.push_str("<pre><code>");
                in_code_block = true;
            }
            continue;
        }

        if in_code_block {
            out.push_str(&escape_html(line));
            out.push('\n');
        } else if line.is_empty() {
            out.push_str("<br>");
        } else {
            out.push_str("<p>");
            out.push_str(&escape_html(line));
            out.push_str("</p>\n");
        }
    }

    if in_code_block {
        out.push_str("</code></pre>\n");
    }

    out
}
