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
    Text source = new Text("<r xmlns='urn:r'><item>café &amp; 🦀</item></r>");
    XmlDocument document = XmlDocument.parse(source)?;
    Text raw = new Text("<&\"' café");
    Text escaped = XmlDocument.escape(raw);
    escaped.print();
    Text root = document.rootName();
    root.print();
    Text prefix = new Text("x");
    Text uri = new Text("urn:r");
    document.registerNamespace(prefix, uri)?;
    Text textPath = new Text("/x:r/x:item/text()");
    Text xmlPath = new Text("/x:r/x:item");
    List<Text> textValues = document.queryText(textPath)?;
    List<Text> xmlValues = document.queryXml(xmlPath)?;
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
    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    assert!(stdout.contains("&lt;&amp;&quot;&apos; café\nr\ncafé & 🦀\n"), "{stdout}");
    assert!(stdout.contains("<item xmlns=\"urn:r\">café &amp; 🦀</item>"), "{stdout}");
    assert_memory_balanced(&output);
}

#[test]
fn mutation_serialize_reparse_and_real_xsd_validation() {
    let source = r#"
Result<Unit, Text> exercise() {
    Text source = new Text("<root xmlns:x='urn:x'><item id='a'><child>old</child></item><item id='b'>stay</item></root>");
    XmlDocument document = XmlDocument.parse(source)?;
    Text childPath = new Text("/root/item[@id='a']/child");
    Text fresh = new Text("fresh");
    println(document.setText(childPath, fresh)?);
    Text prefix = new Text("x");
    Text uri = new Text("urn:x");
    document.registerNamespace(prefix, uri)?;
    Text itemPath = new Text("/root/item[@id='b']");
    Text stateName = new Text("state");
    Text changed = new Text("changed");
    Text flagName = new Text("x:flag");
    Text yes = new Text("yes");
    println(document.setAttribute(itemPath, stateName, changed)?);
    println(document.setAttribute(itemPath, flagName, yes)?);
    Text rootPath = new Text("/root");
    Text extraXml = new Text("<extra>ok &amp; café</extra>");
    println(document.appendXml(rootPath, extraXml)?);
    Text removePath = new Text("/root/item[@id='a'] | /root/item[@id='a']/child");
    println(document.remove(removePath)?);
    Text serialized = document.serialize()?;
    XmlDocument reparsed = XmlDocument.parse(serialized)?;
    Text rootName = reparsed.rootName();
    rootName.print();
    Text statePath = new Text("/root/item[@id='b']/@state");
    List<Text> stateValues = reparsed.queryText(statePath)?;
    reparsed.registerNamespace(prefix, uri)?;
    borrow Text state = stateValues.getRef(0);
    state.print();
    Text flagPath = new Text("/root/item[@id='b']/@x:flag");
    List<Text> flagValues = reparsed.queryText(flagPath)?;
    borrow Text flag = flagValues.getRef(0);
    flag.print();
    Text extraPath = new Text("/root/extra/text()");
    List<Text> extraValues = reparsed.queryText(extraPath)?;
    borrow Text extra = extraValues.getRef(0);
    extra.print();
    Text schema = new Text("<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\"><xs:element name=\"root\"><xs:complexType><xs:sequence><xs:any minOccurs=\"0\" maxOccurs=\"unbounded\" processContents=\"skip\"/></xs:sequence><xs:anyAttribute processContents=\"skip\"/></xs:complexType></xs:element></xs:schema>");
    reparsed.validate(schema)?;
    println("xsd-valid");
    Text invalidSource = new Text("<other/>");
    XmlDocument invalid = XmlDocument.parse(invalidSource)?;
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
    assert_eq!(output.stdout, b"1\n1\n1\n1\n1\nroot\nchanged\nyes\nok & caf\xc3\xa9\nxsd-valid\nxsd-invalid-rejected\n");
    assert_memory_balanced(&output);
}

#[test]
fn malformed_xpath_dtd_xxe_nul_and_invalid_xml_characters_are_recoverable() {
    let source = r#"
Result<Unit, Text> exercise() {
    Text malformed = new Text("<root>");
    match (XmlDocument.parse(malformed)) {
        Ok(document) { drop(document); println("malformed-accepted"); }
        Err(_) { println("malformed-rejected"); }
    }
    Text externalEntity = new Text("<!DOCTYPE root [<!ENTITY secret SYSTEM 'file:///etc/passwd'>]><root>&secret;</root>");
    match (XmlDocument.parse(externalEntity)) {
        Ok(document) { drop(document); println("xxe-accepted"); }
        Err(_) { println("xxe-rejected"); }
    }
    Text nulSource = new Text("<root>\0</root>");
    match (XmlDocument.parse(nulSource)) {
        Ok(document) { drop(document); println("nul-source-accepted"); }
        Err(_) { println("nul-source-rejected"); }
    }
    Text rootSource = new Text("<root/>");
    XmlDocument document = XmlDocument.parse(rootSource)?;
    Text malformedPath = new Text("//*[");
    match (document.queryText(malformedPath)) {
        Ok(values) { drop(values); println("xpath-accepted"); }
        Err(_) { println("xpath-rejected"); }
    }
    Text nulPath = new Text("/root\0");
    match (document.queryText(nulPath)) {
        Ok(values) { drop(values); println("nul-xpath-accepted"); }
        Err(_) { println("nul-xpath-rejected"); }
    }
    Text rootPath = new Text("/root");
    Text invalidCharacter = new Text("");
    invalidCharacter.appendChar('\u{1}');
    match (document.setText(rootPath, invalidCharacter)) {
        Ok(_) { println("invalid-character-accepted"); }
        Err(_) { println("invalid-character-rejected"); }
    }
    Text nulPrefix = new Text("x\0");
    Text prefix = new Text("x");
    Text uri = new Text("urn:x");
    match (document.registerNamespace(nulPrefix, uri)) {
        Ok(_) { println("nul-prefix-accepted"); }
        Err(_) { println("nul-prefix-rejected"); }
    }
    Text nulUri = new Text("urn:x\0");
    match (document.registerNamespace(prefix, nulUri)) {
        Ok(_) { println("nul-uri-accepted"); }
        Err(_) { println("nul-uri-rejected"); }
    }
    Text reservedPrefix = new Text("xml");
    match (document.registerNamespace(reservedPrefix, uri)) {
        Ok(_) { println("reserved-prefix-accepted"); }
        Err(_) { println("reserved-prefix-rejected"); }
    }
    Text reservedUri = new Text("http://www.w3.org/XML/1998/namespace");
    match (document.registerNamespace(prefix, reservedUri)) {
        Ok(_) { println("reserved-uri-accepted"); }
        Err(_) { println("reserved-uri-rejected"); }
    }
    Text xmlnsUri = new Text("http://www.w3.org/2000/xmlns/");
    match (document.registerNamespace(prefix, xmlnsUri)) {
        Ok(_) { println("xmlns-uri-accepted"); }
        Err(_) { println("xmlns-uri-rejected"); }
    }
    Text externalSchema = new Text("<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\"><xs:include schemaLocation=\"file:///etc/passwd\"/></xs:schema>");
    match (document.validate(externalSchema)) {
        Ok(_) { println("external-xsd-accepted"); }
        Err(_) { println("external-xsd-rejected"); }
    }
    Text includeSource = new Text("<root xmlns:xi='http://www.w3.org/2001/XInclude'><xi:include href='file:///etc/passwd' parse='text'/></root>");
    XmlDocument xinclude = XmlDocument.parse(includeSource)?;
    Text includePath = new Text("/root/text()");
    List<Text> includedText = xinclude.queryText(includePath)?;
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
    Text source = new Text("&literal; <café>");
    Text escaped = XmlDocument.escape(source);
    escaped.print();
}
"#);
    assert!(escape_only.status.success(), "{}", String::from_utf8_lossy(&escape_only.stderr));
    assert_eq!(escape_only.stdout, b"&amp;literal; &lt;caf\xc3\xa9&gt;\n");
    assert_memory_balanced(&escape_only);

    let query_after_integer_list = execute(r#"
Result<Unit, Text> exercise() {
    List<int> numbers = new List<int>();
    numbers.add(7);
    Text source = new Text("<root><item>value</item></root>");
    XmlDocument document = XmlDocument.parse(source)?;
    Text textPath = new Text("/root/item/text()");
    Text xmlPath = new Text("/root/item");
    List<Text> values = document.queryText(textPath)?;
    List<Text> markup = document.queryXml(xmlPath)?;
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
    Text source = new Text("<root label='old'><item>old</item></root>");
    XmlDocument values = XmlDocument.parse(source)?;
    Text literal = new Text("&#65; &custom; &amp;");
    Text itemPath = new Text("/root/item | /root/item/text()");
    Text labelPath = new Text("/root/@label");
    Text contentPath = new Text("/root/item/text()");
    println(values.setText(itemPath, literal)?);
    println(values.setText(labelPath, literal)?);
    List<Text> content = values.queryText(contentPath)?;
    List<Text> label = values.queryText(labelPath)?;
    borrow Text contentValue = content.getRef(0);
    borrow Text labelValue = label.getRef(0);
    contentValue.print();
    labelValue.print();
    println(values.remove(itemPath)?);

    Text unboundRoot = new Text("<p:root/>");
    match (XmlDocument.parse(unboundRoot)) {
        Ok(document) { drop(document); println("unbound-root-accepted"); }
        Err(_) { println("unbound-root-rejected"); }
    }
    Text unboundChild = new Text("<root><p:item/></root>");
    match (XmlDocument.parse(unboundChild)) {
        Ok(document) { drop(document); println("unbound-child-accepted"); }
        Err(_) { println("unbound-child-rejected"); }
    }

    Text encodedSource = new Text("<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><root><a xmlns:p=\"urn:p\"/><b/></root>");
    XmlDocument encoded = XmlDocument.parse(encodedSource)?;
    Text destinations = new Text("/root/a | /root/b");
    Text unboundFragment = new Text("<p:item/>");
    match (encoded.appendXml(destinations, unboundFragment)) {
        Ok(_) { println("namespace-fragment-accepted"); }
        Err(_) { println("namespace-fragment-rejected"); }
    }
    Text unchangedPath = new Text("/root/a/* | /root/b/*");
    List<Text> unchanged = encoded.queryXml(unchangedPath)?;
    println(unchanged.size());
    Text appendPath = new Text("/root/b");
    Text fragment = new Text("<item>café</item>");
    match (encoded.appendXml(appendPath, fragment)) {
        Ok(_) { println("utf8-fragment-appended"); }
        Err(_) { println("utf8-fragment-rejected"); }
    }
    Text appendedPath = new Text("/root/b/item/text()");
    List<Text> appended = encoded.queryText(appendedPath)?;
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
