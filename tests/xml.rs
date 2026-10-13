use nacre::{compile_to_c, native};
use std::process::Output;

fn execute(source: &str) -> Output {
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("xml.ncr", source)));
    native::compile_c_with_flags(&c, &["-DNACRE_MEMORY_STATS"])
        .unwrap_or_else(|error| panic!("native compile failed: {error}"))
        .output()
        .unwrap_or_else(|error| panic!("native execution failed: {error}"))
}

fn assert_memory_balanced(output: &Output) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr.lines().find(|line| line.starts_with("NACRE_MEMORY "))
        .unwrap_or_else(|| panic!("missing memory report:\n{stderr}"));
    let mut fields = line.split_whitespace();
    assert_eq!(fields.next(), Some("NACRE_MEMORY"));
    let allocations = fields.next().unwrap().strip_prefix("allocations=").unwrap();
    let frees = fields.next().unwrap().strip_prefix("frees=").unwrap();
    assert_eq!(allocations, frees, "{line}");
    assert_eq!(fields.next(), Some("live=0"), "{line}");
    assert_eq!(stderr.lines().count(), 1, "{stderr}");
    assert_eq!(fields.next(), None, "{line}");
}

#[test]
fn unicode_namespace_xpath_escape_and_owned_query_results() {
    let source = r#"
Result<Unit, Text> exercise() {
    XmlDocument document = XmlDocument.parse(new Text("<r xmlns='urn:r'><item>café &amp; 🦀</item></r>"))?;
    XmlDocument.escape(new Text("<&\"' café")).print();
    document.rootName().print();
    document.registerNamespace(new Text("x"), new Text("urn:r"))?;
    List<Text> textValues = document.queryText(new Text("/x:r/x:item/text()"))?;
    List<Text> xmlValues = document.queryXml(new Text("/x:r/x:item"))?;
    drop(document);
    borrow Text textValue = textValues.getRef(0);
    borrow Text xmlValue = xmlValues.getRef(0);
    textValue.print();
    xmlValue.print();
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (exercise()) {
        Ok(_) { }
        Err(error) { error.print(); }
    }
}
"#;
    let output = execute(source);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("&lt;&amp;&quot;&apos; café\nr\ncafé & 🦀\n"), "{stdout}");
    assert!(stdout.contains("<item xmlns=\"urn:r\">café &amp; 🦀</item>"), "{stdout}");
    assert_memory_balanced(&output);
}

#[test]
fn mutation_serialize_reparse_and_real_xsd_validation() {
    let source = r#"
Result<Unit, Text> exercise() {
    XmlDocument document = XmlDocument.parse(new Text("<root xmlns:x='urn:x'><item id='a'><child>old</child></item><item id='b'>stay</item></root>"))?;
    println(document.setText(new Text("/root/item[@id='a']/child"), new Text("fresh"))?);
    document.registerNamespace(new Text("x"), new Text("urn:x"))?;
    println(document.setAttribute(new Text("/root/item[@id='b']"), new Text("state"), new Text("changed"))?);
    println(document.setAttribute(new Text("/root/item[@id='b']"), new Text("x:flag"), new Text("yes"))?);
    println(document.appendXml(new Text("/root"), new Text("<extra>ok &amp; café</extra>"))?);
    println(document.remove(new Text("/root/item[@id='a'] | /root/item[@id='a']/child"))?);
    Text serialized = document.serialize()?;
    XmlDocument reparsed = XmlDocument.parse(serialized)?;
    reparsed.rootName().print();
    List<Text> stateValues = reparsed.queryText(new Text("/root/item[@id='b']/@state"))?;
    reparsed.registerNamespace(new Text("x"), new Text("urn:x"))?;
    borrow Text state = stateValues.getRef(0);
    state.print();
    List<Text> flagValues = reparsed.queryText(new Text("/root/item[@id='b']/@x:flag"))?;
    borrow Text flag = flagValues.getRef(0);
    flag.print();
    List<Text> extraValues = reparsed.queryText(new Text("/root/extra/text()"))?;
    borrow Text extra = extraValues.getRef(0);
    extra.print();
    Text schema = new Text("<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\"><xs:element name=\"root\"><xs:complexType><xs:sequence><xs:any minOccurs=\"0\" maxOccurs=\"unbounded\" processContents=\"skip\"/></xs:sequence><xs:anyAttribute processContents=\"skip\"/></xs:complexType></xs:element></xs:schema>");
    reparsed.validate(schema)?;
    println("xsd-valid");
    XmlDocument invalid = XmlDocument.parse(new Text("<other/>"))?;
    match (invalid.validate(schema)) {
        Ok(_) { println("xsd-invalid-accepted"); }
        Err(_) { println("xsd-invalid-rejected"); }
    }
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (exercise()) {
        Ok(_) { }
        Err(error) { error.print(); }
    }
}
"#;
    let output = execute(source);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"1\n1\n1\n1\nroot\nchanged\nyes\nok & caf\xc3\xa9\nxsd-valid\nxsd-invalid-rejected\n");
    assert_memory_balanced(&output);
}

#[test]
fn malformed_xpath_dtd_xxe_nul_and_invalid_xml_characters_are_recoverable() {
    let source = r#"
Result<Unit, Text> exercise() {
    match (XmlDocument.parse(new Text("<root>"))) {
        Ok(document) { drop(document); println("malformed-accepted"); }
        Err(_) { println("malformed-rejected"); }
    }
    match (XmlDocument.parse(new Text("<!DOCTYPE root [<!ENTITY secret SYSTEM 'file:///etc/passwd'>]><root>&secret;</root>"))) {
        Ok(document) { drop(document); println("xxe-accepted"); }
        Err(_) { println("xxe-rejected"); }
    }
    match (XmlDocument.parse(new Text("<root>\0</root>"))) {
        Ok(document) { drop(document); println("nul-source-accepted"); }
        Err(_) { println("nul-source-rejected"); }
    }
    XmlDocument document = XmlDocument.parse(new Text("<root/>"))?;
    match (document.queryText(new Text("//*["))) {
        Ok(values) { drop(values); println("xpath-accepted"); }
        Err(_) { println("xpath-rejected"); }
    }
    match (document.queryText(new Text("/root\0"))) {
        Ok(values) { drop(values); println("nul-xpath-accepted"); }
        Err(_) { println("nul-xpath-rejected"); }
    }
    match (document.setText(new Text("/root"), new Text("\u{1}"))) {
        Ok(_) { println("invalid-character-accepted"); }
        Err(_) { println("invalid-character-rejected"); }
    }
    match (document.registerNamespace(new Text("x\0"), new Text("urn:x"))) {
        Ok(_) { println("nul-prefix-accepted"); }
        Err(_) { println("nul-prefix-rejected"); }
    }
    match (document.registerNamespace(new Text("x"), new Text("urn:x\0"))) {
        Ok(_) { println("nul-uri-accepted"); }
        Err(_) { println("nul-uri-rejected"); }
    }
    match (document.registerNamespace(new Text("xml"), new Text("urn:x"))) {
        Ok(_) { println("reserved-prefix-accepted"); }
        Err(_) { println("reserved-prefix-rejected"); }
    }
    match (document.registerNamespace(new Text("x"), new Text("http://www.w3.org/XML/1998/namespace"))) {
        Ok(_) { println("reserved-uri-accepted"); }
        Err(_) { println("reserved-uri-rejected"); }
    }
    match (document.registerNamespace(new Text("x"), new Text("http://www.w3.org/2000/xmlns/"))) {
        Ok(_) { println("xmlns-uri-accepted"); }
        Err(_) { println("xmlns-uri-rejected"); }
    }
    match (document.validate(new Text("<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\"><xs:include schemaLocation=\"file:///etc/passwd\"/></xs:schema>"))) {
        Ok(_) { println("external-xsd-accepted"); }
        Err(_) { println("external-xsd-rejected"); }
    }
    XmlDocument xinclude = XmlDocument.parse(new Text("<root xmlns:xi='http://www.w3.org/2001/XInclude'><xi:include href='file:///etc/passwd' parse='text'/></root>"))?;
    List<Text> includedText = xinclude.queryText(new Text("/root/text()"))?;
    println(includedText.size());
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (exercise()) {
        Ok(_) { }
        Err(error) { error.print(); }
    }
}
"#;
    let output = execute(source);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout,
        b"malformed-rejected\nxxe-rejected\nnul-source-rejected\nxpath-rejected\nnul-xpath-rejected\ninvalid-character-rejected\nnul-prefix-rejected\nnul-uri-rejected\nreserved-prefix-rejected\nreserved-uri-rejected\nxmlns-uri-rejected\nexternal-xsd-rejected\n0\n");
    assert_memory_balanced(&output);
}

#[test]
fn escape_only_and_text_query_after_integer_list_specialization() {
    let escape_only = execute(r#"
void main() {
    XmlDocument.escape(new Text("&literal; <café>")).print();
}
"#);
    assert!(escape_only.status.success(), "{}", String::from_utf8_lossy(&escape_only.stderr));
    assert_eq!(escape_only.stdout, b"&amp;literal; &lt;caf\xc3\xa9&gt;\n");
    assert_memory_balanced(&escape_only);

    let query_after_integer_list = execute(r#"
Result<Unit, Text> exercise() {
    List<int> numbers = new List<int>();
    numbers.add(7);
    XmlDocument document = XmlDocument.parse(new Text("<root><item>value</item></root>"))?;
    List<Text> values = document.queryText(new Text("/root/item/text()"))?;
    List<Text> markup = document.queryXml(new Text("/root/item"))?;
    println(numbers.get(0));
    borrow Text value = values.getRef(0);
    borrow Text xml = markup.getRef(0);
    value.print();
    xml.print();
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (exercise()) {
        Ok(_) { }
        Err(error) { error.print(); }
    }
}
"#);
    assert!(query_after_integer_list.status.success(),
        "{}", String::from_utf8_lossy(&query_after_integer_list.stderr));
    assert_eq!(query_after_integer_list.stdout, b"7\nvalue\n<item>value</item>\n");
    assert_memory_balanced(&query_after_integer_list);
}

#[test]
fn literal_set_text_namespace_atomicity_and_utf8_fragments() {
    let output = execute(r#"
Result<Unit, Text> exercise() {
    XmlDocument values = XmlDocument.parse(new Text("<root label='old'><item>old</item></root>"))?;
    Text literal = new Text("&#65; &custom; &amp;");
    println(values.setText(new Text("/root/item | /root/item/text()"), literal)?);
    println(values.setText(new Text("/root/@label"), literal)?);
    List<Text> content = values.queryText(new Text("/root/item/text()"))?;
    List<Text> label = values.queryText(new Text("/root/@label"))?;
    borrow Text contentValue = content.getRef(0);
    borrow Text labelValue = label.getRef(0);
    contentValue.print();
    labelValue.print();
    println(values.remove(new Text("/root/item | /root/item/text()"))?);

    match (XmlDocument.parse(new Text("<p:root/>"))) {
        Ok(document) { drop(document); println("unbound-root-accepted"); }
        Err(_) { println("unbound-root-rejected"); }
    }
    match (XmlDocument.parse(new Text("<root><p:item/></root>"))) {
        Ok(document) { drop(document); println("unbound-child-accepted"); }
        Err(_) { println("unbound-child-rejected"); }
    }

    XmlDocument encoded = XmlDocument.parse(new Text("<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><root><a xmlns:p=\"urn:p\"/><b/></root>"))?;
    match (encoded.appendXml(new Text("/root/a | /root/b"), new Text("<p:item/>"))) {
        Ok(_) { println("namespace-fragment-accepted"); }
        Err(_) { println("namespace-fragment-rejected"); }
    }
    List<Text> unchanged = encoded.queryXml(new Text("/root/a/* | /root/b/*"))?;
    println(unchanged.size());
    match (encoded.appendXml(new Text("/root/b"), new Text("<item>café</item>"))) {
        Ok(_) { println("utf8-fragment-appended"); }
        Err(_) { println("utf8-fragment-rejected"); }
    }
    List<Text> appended = encoded.queryText(new Text("/root/b/item/text()"))?;
    borrow Text appendedValue = appended.getRef(0);
    appendedValue.print();
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (exercise()) {
        Ok(_) { }
        Err(error) { error.print(); }
    }
}
"#);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout,
        b"1\n1\n&#65; &custom; &amp;\n&#65; &custom; &amp;\n1\nunbound-root-rejected\nunbound-child-rejected\nnamespace-fragment-rejected\n0\nutf8-fragment-appended\ncaf\xc3\xa9\n");
    assert_memory_balanced(&output);
}
