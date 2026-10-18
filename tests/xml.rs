mod support;

use nacre::native;
use std::process::Output;

fn execute(source: &str) -> Output {
    support::compile(source, &["-DNACRE_MEMORY_STATS"])
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

#[test]
fn pull_reader_keeps_owned_events_and_distinguishes_truncation_from_eof() {
    let output = execute(r#"
Result<Unit, Text> exercise() {
    Text source = new Text("<r xmlns='urn:r' xmlns:x='urn:x'><x:item id='a' x:flag='v'>text<![CDATA[raw]]><!--note--><?pi data?></x:item><empty/></r>");
    XmlReader reader = XmlReader.fromText(source)?;
    List<XmlEvent> retained = new List<XmlEvent>();
    int elements = 0;
    int texts = 0;
    int cdatas = 0;
    int comments = 0;
    int instructions = 0;
    int emptyElements = 0;
    boolean foundId = false;
    boolean foundFlag = false;
    boolean boundsRejected = false;
    int eventCount = 0;
    while (true) {
        match (reader.read()?) {
            Some(event) {
                int kind = event.nodeType();
                if (kind == 1) {
                    elements = elements + 1;
                    Text localName = event.localName();
                    Text elementNamespace = event.namespaceUri();
                    if (localName.equalsString("empty") && event.isEmptyElement()) {
                        emptyElements = emptyElements + 1;
                    }
                    if (localName.equalsString("item") &&
                        elementNamespace.equalsString("urn:x")) {
                        int index = 0;
                        while (index < event.attributeCount()) {
                            Text name = event.attributeName(index)?;
                            Text namespaceUri = event.attributeNamespaceUri(index)?;
                            Text value = event.attributeValue(index)?;
                            if (name.equalsString("id") && namespaceUri.isEmpty() &&
                                value.equalsString("a")) { foundId = true; }
                            if (name.equalsString("x:flag") &&
                                namespaceUri.equalsString("urn:x") && value.equalsString("v")) {
                                foundFlag = true;
                            }
                            index = index + 1;
                        }
                        match (event.attributeName(event.attributeCount())) {
                            Ok(value) { drop(value); }
                            Err(_) { boundsRejected = true; }
                        }
                    }
                }
                if (kind == 3 || kind == 4 || kind == 8 || kind == 7) {
                    Text value = event.value();
                    if (kind == 3 && value.equalsString("text")) { texts = texts + 1; }
                    if (kind == 4 && value.equalsString("raw")) { cdatas = cdatas + 1; }
                    if (kind == 8 && value.equalsString("note")) { comments = comments + 1; }
                    if (kind == 7 && value.equalsString("data")) { instructions = instructions + 1; }
                }
                if (eventCount == 0) { retained.add(event); }
                else { drop(event); }
                eventCount = eventCount + 1;
            }
            None { break; }
        }
    }
    reader.close()?;
    reader.close()?;
    borrow XmlEvent first = retained.getRef(0);
    Text firstName = first.localName();
    if (firstName.equalsString("r")) { println("snapshot-stable"); }
    println(elements);
    println(texts);
    println(cdatas);
    println(comments);
    println(instructions);
    println(emptyElements);
    if (boundsRejected) { println("attribute-bounds-rejected"); }
    if (foundId) { println("unqualified-attribute-ok"); }
    if (foundFlag) { println("qualified-attribute-ok"); }
    match (reader.read()) {
        Ok(_) { println("closed-read-accepted"); }
        Err(_) { println("closed-read-rejected"); }
    }

    Text truncatedSource = new Text("<broken>");
    XmlReader truncated = XmlReader.fromText(truncatedSource)?;
    while (true) {
        match (truncated.read()) {
            Ok(next) {
                match (next) {
                    Some(event) { drop(event); }
                    None { println("truncated-as-eof"); break; }
                }
            }
            Err(_) { println("truncated-error"); break; }
        }
    }
    match (truncated.read()) {
        Err(_) { println("truncated-error-sticky"); }
        Ok(_) { println("truncated-error-lost"); }
    }
    Text dtdSource = new Text("<!DOCTYPE root [<!ENTITY marker 'expanded'>]><root>&marker;</root>");
    XmlReader dtd = XmlReader.fromText(dtdSource)?;
    match (dtd.read()) {
        Err(_) { println("reader-dtd-rejected"); }
        Ok(_) { println("reader-dtd-accepted"); }
    }
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
        b"snapshot-stable\n3\n1\n1\n1\n1\n1\nattribute-bounds-rejected\nunqualified-attribute-ok\nqualified-attribute-ok\nclosed-read-rejected\ntruncated-error\ntruncated-error-sticky\nreader-dtd-rejected\n");
    assert_memory_balanced(&output);
}

#[test]
fn streaming_writer_escapes_namespaces_and_rejects_unbalanced_or_late_operations() {
    let output = execute(r#"
Result<Unit, Text> exercise() {
    XmlWriter writer = XmlWriter.create()?;
    Text premature = new Text("premature");
    match (writer.startElement(premature)) {
        Ok(_) { println("premature-start-accepted"); }
        Err(_) { println("premature-start-rejected"); }
    }
    writer.startDocument()?;
    Text before = new Text("before");
    Text data = new Text("data");
    Text empty = new Text("");
    Text root = new Text("root");
    Text rootNamespace = new Text("urn:r");
    Text note = new Text("note");
    Text quoted = new Text("a&\"");
    Text plain = new Text("plain");
    Text x = new Text("x");
    Text item = new Text("item");
    Text itemNamespace = new Text("urn:x");
    Text p = new Text("p");
    Text flag = new Text("flag");
    Text flagNamespace = new Text("urn:flag");
    Text attributeValue = new Text("v&");
    Text content = new Text("left<&");
    Text late = new Text("late");
    Text no = new Text("no");
    writer.writeComment(before)?;
    writer.writeProcessingInstruction(before, data)?;
    writer.startElementNs(empty, root, rootNamespace)?;
    writer.writeAttribute(note, quoted)?;
    writer.startElement(plain)?;
    writer.startElementNs(x, item, itemNamespace)?;
    writer.writeAttributeNs(p, flag, flagNamespace, attributeValue)?;
    writer.writeText(content)?;
    match (writer.writeAttribute(late, no)) {
        Ok(_) { println("late-attribute-accepted"); }
        Err(_) { println("late-attribute-rejected"); }
    }
    Text raw = new Text("raw<");
    Text inside = new Text("inside");
    Text step = new Text("step");
    writer.writeCData(raw)?;
    writer.writeComment(inside)?;
    writer.writeProcessingInstruction(step, data)?;
    writer.endElement()?;
    match (writer.finish()) {
        Ok(bytes) { drop(bytes); println("unbalanced-finish-accepted"); }
        Err(_) { println("unbalanced-finish-rejected"); }
    }
    writer.endElement()?;
    writer.endElement()?;
    Text secondRoot = new Text("second-root");
    match (writer.startElement(secondRoot)) {
        Ok(_) { println("second-root-accepted"); }
        Err(_) { println("second-root-rejected"); }
    }
    Text after = new Text("after");
    writer.writeComment(after)?;
    ByteBuffer encoded = writer.finish()?;
    XmlReader reader = XmlReader.fromBuffer(encoded)?;
    drop(encoded);

    boolean rootOk = false;
    boolean plainOk = false;
    boolean itemOk = false;
    boolean textOk = false;
    boolean cdataOk = false;
    boolean commentOk = false;
    boolean instructionOk = false;
    boolean rootAttributeOk = false;
    boolean namespacedAttributeOk = false;
    while (true) {
        match (reader.read()?) {
            Some(event) {
                Text localName = event.localName();
                Text elementNamespace = event.namespaceUri();
                if (event.nodeType() == 1 && event.depth() == 0 &&
                    localName.equalsString("root") &&
                    elementNamespace.equalsString("urn:r")) {
                    rootOk = true;
                    int index = 0;
                    while (index < event.attributeCount()) {
                        Text name = event.attributeName(index)?;
                        Text value = event.attributeValue(index)?;
                        if (name.equalsString("note") && value.equalsString("a&\"")) {
                            rootAttributeOk = true;
                        }
                        index = index + 1;
                    }
                }
                if (event.nodeType() == 1 && localName.equalsString("plain") &&
                    elementNamespace.isEmpty()) { plainOk = true; }
                if (event.nodeType() == 1 && localName.equalsString("item") &&
                    elementNamespace.equalsString("urn:x")) {
                    itemOk = true;
                    int index = 0;
                    while (index < event.attributeCount()) {
                        Text name = event.attributeName(index)?;
                        Text namespaceUri = event.attributeNamespaceUri(index)?;
                        Text value = event.attributeValue(index)?;
                        if (name.equalsString("p:flag") &&
                            namespaceUri.equalsString("urn:flag") && value.equalsString("v&")) {
                            namespacedAttributeOk = true;
                        }
                        index = index + 1;
                    }
                }
                if (event.nodeType() == 3 || event.nodeType() == 4 ||
                    event.nodeType() == 8 || event.nodeType() == 7) {
                    Text value = event.value();
                    if (event.nodeType() == 3 && value.equalsString("left<&")) { textOk = true; }
                    if (event.nodeType() == 4 && value.equalsString("raw<")) { cdataOk = true; }
                    if (event.nodeType() == 8 && value.equalsString("inside")) { commentOk = true; }
                    if (event.nodeType() == 7 && value.equalsString("data")) { instructionOk = true; }
                }
                drop(event);
            }
            None { break; }
        }
    }
    reader.close()?;
    if (rootOk) { println("default-namespace-ok"); }
    if (plainOk) { println("default-namespace-reset-ok"); }
    if (itemOk) { println("prefixed-element-ok"); }
    if (textOk) { println("escaped-text-roundtrip-ok"); }
    if (cdataOk) { println("cdata-roundtrip-ok"); }
    if (commentOk) { println("comment-roundtrip-ok"); }
    if (instructionOk) { println("pi-roundtrip-ok"); }
    if (rootAttributeOk) { println("escaped-attribute-roundtrip-ok"); }
    if (namespacedAttributeOk) { println("namespaced-attribute-ok"); }
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
        b"premature-start-rejected\nlate-attribute-rejected\nunbalanced-finish-rejected\nsecond-root-rejected\ndefault-namespace-ok\ndefault-namespace-reset-ok\nprefixed-element-ok\nescaped-text-roundtrip-ok\ncdata-roundtrip-ok\ncomment-roundtrip-ok\npi-roundtrip-ok\nescaped-attribute-roundtrip-ok\nnamespaced-attribute-ok\n");
    assert_memory_balanced(&output);
}

#[test]
fn xslt_transforms_owned_documents_and_blocks_external_resources_and_extensions() {
    struct ExternalStylesheet(std::path::PathBuf);
    impl Drop for ExternalStylesheet {
        fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); }
    }

    let path = std::env::temp_dir().join(format!("nacre-xslt-resource-{}.xml", std::process::id()));
    std::fs::write(&path, br#"<xsl:stylesheet xmlns:xsl="http://www.w3.org/1999/XSL/Transform" version="1.0"><xsl:template match="/"><external>leaked</external></xsl:template></xsl:stylesheet>"#)
        .unwrap();
    let _external = ExternalStylesheet(path.clone());
    let file_uri = format!("file://{}", path.to_string_lossy());
    let source = r#"
Result<Unit, Text> exercise() {
    Text source = new Text("<root><item>local</item></root>");
    XmlDocument input = XmlDocument.parse(source)?;
    Text localSource = new Text("<xsl:stylesheet xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\" version=\"1.0\"><xsl:template match=\"/\"><out><xsl:value-of select=\"/root/item\"/></out></xsl:template></xsl:stylesheet>");
    XsltStylesheet local = XsltStylesheet.compile(localSource)?;
    XmlDocument transformed = local.transform(input)?;
    Text outputPath = new Text("/out/text()");
    List<Text> localValues = transformed.queryText(outputPath)?;
    borrow Text localValue = localValues.getRef(0);
    localValue.print();
    Text inputPath = new Text("/root/item/text()");
    List<Text> sourceValues = input.queryText(inputPath)?;
    borrow Text sourceValue = sourceValues.getRef(0);
    sourceValue.print();

    Text externalSource = new Text("<xsl:stylesheet xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\" version=\"1.0\"><xsl:template match=\"/\"><out><xsl:value-of select=\"document('@XML_URI@')/xsl:stylesheet/xsl:template/external\"/></out></xsl:template></xsl:stylesheet>");
    XsltStylesheet externalRead = XsltStylesheet.compile(externalSource)?;
    match (externalRead.transform(input)) {
        Ok(result) { drop(result); println("external-document-accepted"); }
        Err(_) { println("external-document-rejected"); }
    }
    Text includedSource = new Text("<xsl:stylesheet xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\" version=\"1.0\"><xsl:include href=\"@XML_URI@\"/></xsl:stylesheet>");
    match (XsltStylesheet.compile(includedSource)) {
        Ok(style) { drop(style); println("external-include-accepted"); }
        Err(_) { println("external-include-rejected"); }
    }
    Text extensionSource = new Text("<xsl:stylesheet xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\" xmlns:e=\"urn:blocked\" version=\"1.0\" extension-element-prefixes=\"e\"><xsl:template match=\"/\"><e:write/></xsl:template></xsl:stylesheet>");
    match (XsltStylesheet.compile(extensionSource)) {
        Ok(style) { drop(style); println("extension-element-accepted"); }
        Err(_) { println("extension-element-rejected"); }
    }
    Text extensionCallSource = new Text("<xsl:stylesheet xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\" xmlns:e=\"urn:blocked\" version=\"1.0\"><xsl:template match=\"/\"><out><xsl:value-of select=\"e:read()\"/></out></xsl:template></xsl:stylesheet>");
    match (XsltStylesheet.compile(extensionCallSource)) {
        Ok(extensionCall) {
            match (extensionCall.transform(input)) {
                Ok(result) { drop(result); println("extension-function-accepted"); }
                Err(_) { println("extension-function-rejected"); }
            }
        }
        Err(_) { println("extension-function-rejected"); }
    }
    drop(input);
    drop(local);
    List<Text> retainedResult = transformed.queryText(outputPath)?;
    borrow Text retainedValue = retainedResult.getRef(0);
    if (retainedValue.equalsString("local")) { println("result-independent"); }
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (exercise()) {
        Ok(_) { }
        Err(error) { error.print(); }
    }
}
"#.replace("@XML_URI@", &file_uri);
    let output = execute(&source);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout,
        b"local\nlocal\nexternal-document-rejected\nexternal-include-rejected\nextension-element-rejected\nextension-function-rejected\nresult-independent\n");
    assert_memory_balanced(&output);
}

#[test]
fn streaming_namespace_errors_are_sticky_and_prefix_conflicts_preserve_writer() {
    let output = execute(r#"
Result<Unit, Text> exercise() {
    Text invalidSource = new Text("<p:root/>");
    XmlReader invalid = XmlReader.fromText(invalidSource)?;
    match (invalid.read()) {
        Err(_) { println("namespace-rejected"); }
        Ok(_) { println("namespace-accepted"); }
    }
    match (invalid.read()) {
        Err(_) { println("namespace-error-sticky"); }
        Ok(_) { println("namespace-error-lost"); }
    }
    invalid.close()?;
    XmlWriter writer = XmlWriter.create()?;
    writer.startDocument()?;
    Text p = new Text("p");
    Text q = new Text("q");
    Text root = new Text("root");
    Text first = new Text("first");
    Text second = new Text("second");
    Text a = new Text("urn:a");
    Text b = new Text("urn:b");
    Text value = new Text("v");
    writer.startElementNs(p, root, a)?;
    match (writer.writeAttributeNs(p, first, b, value)) {
        Err(_) { println("element-prefix-conflict-rejected"); }
        Ok(_) { println("element-prefix-conflict-accepted"); }
    }
    writer.writeAttributeNs(q, first, a, value)?;
    match (writer.writeAttributeNs(q, second, b, value)) {
        Err(_) { println("attribute-prefix-conflict-rejected"); }
        Ok(_) { println("attribute-prefix-conflict-accepted"); }
    }
    writer.writeAttributeNs(q, second, a, value)?;
    writer.endElement()?;
    ByteBuffer bytes = writer.finish()?;
    XmlReader reader = XmlReader.fromBuffer(bytes)?;
    drop(bytes);
    boolean firstFound = false;
    boolean secondFound = false;
    while (true) {
        match (reader.read()?) {
            Some(event) {
                if (event.nodeType() == 1) {
                    Text namespaceUri = event.namespaceUri();
                    namespaceUri.print();
                    int index = 0;
                    while (index < event.attributeCount()) {
                        Text name = event.attributeName(index)?;
                        Text attributeNamespace = event.attributeNamespaceUri(index)?;
                        Text attributeValue = event.attributeValue(index)?;
                        if (attributeNamespace.equalsString("urn:a") &&
                            attributeValue.equalsString("v")) {
                            if (name.equalsString("q:first")) { firstFound = true; }
                            if (name.equalsString("q:second")) { secondFound = true; }
                        }
                        index = index + 1;
                    }
                }
            }
            None { break; }
        }
    }
    println(firstFound);
    println(secondFound);
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (exercise()) {
        Ok(_) {}
        Err(error) { error.print(); }
    }
}
"#);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"namespace-rejected\nnamespace-error-sticky\nelement-prefix-conflict-rejected\nattribute-prefix-conflict-rejected\nurn:a\ntrue\ntrue\n");
    assert_memory_balanced(&output);
}

#[test]
fn xslt_preserves_borrowed_source_rejects_non_documents_and_frees_compile_failures() {
    let source = r#"
Result<Unit, Text> exercise() {
    Text source = new Text("<root> <item/> </root>");
    XmlDocument document = XmlDocument.parse(source)?;
    Text path = new Text("/root/text()");
    List<Text> before = document.queryText(path)?;
    println(before.size());
    Text styleSource = new Text("<xsl:stylesheet xmlns:xsl='http://www.w3.org/1999/XSL/Transform' version='1.0'><xsl:strip-space elements='*'/><xsl:template match='/'><out/></xsl:template></xsl:stylesheet>");
    XsltStylesheet style = XsltStylesheet.compile(styleSource)?;
    XmlDocument result = style.transform(document)?;
    List<Text> after = document.queryText(path)?;
    println(after.size());
    Text manyRootsSource = new Text("<xsl:stylesheet xmlns:xsl='http://www.w3.org/1999/XSL/Transform' version='1.0'><xsl:template match='/'><first/><second/></xsl:template></xsl:stylesheet>");
    XsltStylesheet manyRoots = XsltStylesheet.compile(manyRootsSource)?;
    match (manyRoots.transform(document)) {
        Err(_) { println("many-roots-rejected"); }
        Ok(_) { println("many-roots-accepted"); }
    }
    Text topTextSource = new Text("<xsl:stylesheet xmlns:xsl='http://www.w3.org/1999/XSL/Transform' version='1.0'><xsl:template match='/'><xsl:text>outside</xsl:text><root/></xsl:template></xsl:stylesheet>");
    XsltStylesheet topText = XsltStylesheet.compile(topTextSource)?;
    match (topText.transform(document)) {
        Err(_) { println("top-text-rejected"); }
        Ok(_) { println("top-text-accepted"); }
    }
    List<Text> afterFailure = document.queryText(path)?;
    println(afterFailure.size());
    Text invalid = new Text("<root/>");
    match (XsltStylesheet.compile(invalid)) {
        Err(_) { println("invalid-stylesheet-rejected"); }
        Ok(_) { println("invalid-stylesheet-accepted"); }
    }
    drop(document);
    drop(style);
    Text serialized = result.serialize()?;
    XmlDocument reparsed = XmlDocument.parse(serialized)?;
    Text root = reparsed.rootName();
    root.print();
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (exercise()) {
        Ok(_) {}
        Err(error) { error.print(); }
    }
}
"#;
    let output = support::compile(source, &[
        "-DNACRE_MEMORY_STATS", "-fsanitize=address,undefined",
        "-fno-omit-frame-pointer", "-fno-pie", "-no-pie",
    ]).unwrap_or_else(|error| panic!("native compile failed: {error}"))
        .output().expect("native XSLT regression execution");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"2\n2\nmany-roots-rejected\ntop-text-rejected\n2\ninvalid-stylesheet-rejected\nout\n");
    assert_memory_balanced(&output);
}

#[test]
fn xslt_availability_never_initializes_external_plugins() {
    let source = r#"
Result<Unit, Text> exercise() {
    Text source = new Text("<root/>");
    XmlDocument document = XmlDocument.parse(source)?;
    Text styleSource = new Text("<xsl:stylesheet xmlns:xsl='http://www.w3.org/1999/XSL/Transform' xmlns:e='https://nacre.invalid/probe' version='1.0'><xsl:template match='/'><out><xsl:value-of select=\"concat(element-available('e:probe'),'/',element-available('xsl:value-of'),'/',element-available('xsl:when'),'/',element-available('xsl:otherwise'),'/',element-available('xsl:sort'),'/',element-available('xsl:with-param'),'/',function-available('string'),'/',function-available('e:probe'),'/',element-available('xsl:unknown'))\"/></out></xsl:template></xsl:stylesheet>");
    XsltStylesheet style = XsltStylesheet.compile(styleSource)?;
    XmlDocument result = style.transform(document)?;
    Text path = new Text("/out/text()");
    List<Text> values = result.queryText(path)?;
    borrow Text value = values.getRef(0);
    value.print();
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (exercise()) {
        Ok(_) {}
        Err(error) { error.print(); }
    }
}
"#;
    let program = support::compile(source, &["-DNACRE_MEMORY_STATS"])
        .expect("native availability consumer");
    let directory = program.path().parent().expect("private native directory");
    let plugin_directory = directory.join("plugins");
    std::fs::create_dir(&plugin_directory).expect("private plugin directory");
    let plugin_source = directory.join("plugin.c");
    let plugin_library = plugin_directory.join("nacre_invalid_probe.so");
    let marker = directory.join("plugin-initialized");
    std::fs::write(&plugin_source, r#"
#include <stdio.h>
#include <stdlib.h>
void nacre_invalid_probe_init(void) {
    const char *path = getenv("NACRE_XSLT_PROBE_MARKER");
    if (path == NULL) { return; }
    FILE *file = fopen(path, "wb");
    if (file != NULL) { fputs("plugin-initialized\n", file); fclose(file); }
}
"#).expect("plugin sentinel source");
    let compile = std::process::Command::new(native::find_c_compiler().expect("native compiler"))
        .args(["-std=c11", "-shared", "-fPIC"])
        .arg(&plugin_source).arg("-o").arg(&plugin_library)
        .output().expect("plugin compilation");
    assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
    let output = std::process::Command::new(program.path())
        .env("LIBXSLT_PLUGINS_PATH", &plugin_directory)
        .env("NACRE_XSLT_PROBE_MARKER", &marker)
        .output().expect("restricted availability consumer");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"false/true/true/true/true/true/true/false/false\n");
    assert!(!marker.exists(), "element availability initialized an external plugin");
    assert_memory_balanced(&output);
}

#[test]
fn xslt_element_available_uses_no_namespace_for_unprefixed_qnames() {
    let source = r#"
Result<Unit, Text> exercise() {
    Text input = new Text("<root/>");
    XmlDocument document = XmlDocument.parse(input)?;
    Text styleSource = new Text("<xsl:stylesheet xmlns:xsl='http://www.w3.org/1999/XSL/Transform' xmlns='http://www.w3.org/1999/XSL/Transform' version='1.0'><xsl:template match='/'><xsl:element name='out' namespace=''><xsl:value-of select=\"element-available('value-of')\"/></xsl:element></xsl:template></xsl:stylesheet>");
    XsltStylesheet style = XsltStylesheet.compile(styleSource)?;
    XmlDocument result = style.transform(document)?;
    Text path = new Text("/out/text()");
    List<Text> values = result.queryText(path)?;
    borrow Text value = values.getRef(0);
    value.print();
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (exercise()) {
        Ok(_) {}
        Err(error) { error.print(); }
    }
}
"#;
    let output = execute(source);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"false\n");
    assert_memory_balanced(&output);
}

#[test]
fn xslt_serialized_results_preserve_output_method_encoding_and_owned_lifetimes() {
    let source = r#"
Result<ByteBuffer, Text> render(borrow XmlDocument document, String method, String encoding, String body) {
    Text source = new Text("<xsl:stylesheet xmlns:xsl='http://www.w3.org/1999/XSL/Transform' version='1.0'><xsl:strip-space elements='*'/><xsl:output indent='no' method='");
    source.append(method);
    source.append("' encoding='");
    source.append(encoding);
    source.append("'/><xsl:template match='/'>");
    source.append(body);
    source.append("</xsl:template></xsl:stylesheet>");
    XsltStylesheet stylesheet = XsltStylesheet.compile(source)?;
    return stylesheet.transformBytes(document);
}
Result<Unit, Text> exercise() {
    Text input = new Text("<root>  <value>café 🦀</value>  </root>");
    XmlDocument document = XmlDocument.parse(input)?;
    ByteBuffer plain = render(document, "text", "UTF-8", "<xsl:value-of select='root/value'/>")?;
    ByteBuffer wide = render(document, "text", "UTF-16LE", "<xsl:value-of select='root/value'/>")?;
    ByteBuffer html = render(document, "html", "UTF-8", "<html><body><br/><xsl:value-of select='root/value'/></body></html>")?;
    ByteBuffer xml = render(document, "xml", "UTF-8", "<out><xsl:value-of select='root/value'/></out>")?;
    ByteBuffer empty = render(document, "text", "UTF-8", "")?;
    boolean unsupported = false;
    match (render(document, "text", "nacre-invalid-encoding", "<xsl:value-of select='root/value'/>")) {
        Err(error) { unsupported = true; }
        Ok(bytes) { }
    }
    println(unsupported);
    boolean externalDenied = false;
    match (render(document, "text", "UTF-8", "<xsl:value-of select=\"document('file:///nacre-must-not-be-read')\"/>")) {
        Err(error) { externalDenied = true; }
        Ok(bytes) { }
    }
    println(externalDenied);
    Text whitespacePath = new Text("/root/text()");
    List<Text> whitespace = document.queryText(whitespacePath)?;
    println(whitespace.size() == 2);
    drop(document);
    drop(input);
    Text utf8 = new Text("UTF-8");
    Text utf16le = new Text("UTF-16LE");
    Text plainText = Charset.decode(plain, utf8)?;
    Text wideText = Charset.decode(wide, utf16le)?;
    println(plainText.equalsString("café 🦀"));
    println(wideText.equalsString("café 🦀") && wide.length() == 14L
        && wide.get(0L) == (byte)99 && wide.get(1L) == (byte)0
        && wide.get(12L) == (byte)128 && wide.get(13L) == (byte)221);
    Text htmlText = Charset.decode(html, utf8)?;
    Text breakElement = new Text("<br>");
    Text value = new Text("café 🦀");
    println(htmlText.indexOf(breakElement) >= 0 && htmlText.indexOf(value) >= 0);
    Text xmlText = Charset.decode(xml, utf8)?;
    XmlDocument result = XmlDocument.parse(xmlText)?;
    Text valuePath = new Text("/out/text()");
    List<Text> values = result.queryText(valuePath)?;
    borrow Text resultValue = values.getRef(0);
    println(resultValue.equalsString("café 🦀"));
    println(empty.length() == 0L);
    Text repeated = new Text("a");
    int index = 0;
    while (index < 4500) { repeated.appendChar('🦀'); index = index + 1; }
    Text largeSource = new Text("<root>");
    largeSource.appendText(repeated);
    largeSource.append("</root>");
    XmlDocument largeDocument = XmlDocument.parse(largeSource)?;
    ByteBuffer largeWide = render(largeDocument, "text", "UTF-16LE", "<xsl:value-of select='root'/>")?;
    drop(largeDocument);
    drop(largeSource);
    Text largeText = Charset.decode(largeWide, utf16le)?;
    println(largeWide.length() == 18002L && largeText.equals(repeated));
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (exercise()) {
        Ok(unit) { }
        Err(error) { error.print(); }
    }
}
"#;
    let output = execute(source);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n");
    assert_memory_balanced(&output);
}

#[test]
fn xslt_text_encoding_is_strict_and_flushes_shift_state() {
    let source = r#"
Result<ByteBuffer, Text> render(borrow XmlDocument document, String method, String encoding, String body) {
    Text source = new Text("<xsl:stylesheet xmlns:xsl='http://www.w3.org/1999/XSL/Transform' xmlns:p='urn:unsupported' version='1.0'><xsl:output indent='no' method='");
    source.append(method); source.append("' encoding='"); source.append(encoding);
    source.append("'/><xsl:template match='/'>"); source.append(body);
    source.append("</xsl:template></xsl:stylesheet>");
    XsltStylesheet stylesheet = XsltStylesheet.compile(source)?;
    return stylesheet.transformBytes(document);
}
Result<Unit, Text> exercise() {
    Text input = new Text("<root/>");
    XmlDocument document = XmlDocument.parse(input)?;
    ByteBuffer alias = render(document, "text", "unicode-1-1-utf-8", "<xsl:text>é</xsl:text>")?;
    ByteBuffer unicode11 = render(document, "text", "unicode11utf8", "<xsl:text>é</xsl:text>")?;
    ByteBuffer unicode20 = render(document, "text", "unicode20utf8", "<xsl:text>é</xsl:text>")?;
    ByteBuffer prefixed = render(document, "text", "X-Unicode20UTF8", "<xsl:text>é</xsl:text>")?;
    boolean malformedAlias = false;
    match (render(document, "text", "unicode-1-1-utf-8-extra", "<xsl:text>é</xsl:text>")) {
        Ok(bytes) {} Err(error) { malformedAlias = true; }
    }
    println(alias.length() == 2L && alias.get(0L) == (byte)195 && alias.get(1L) == (byte)169
        && unicode11.length() == 2L && unicode11.get(0L) == (byte)195 && unicode11.get(1L) == (byte)169
        && unicode20.length() == 2L && unicode20.get(0L) == (byte)195 && unicode20.get(1L) == (byte)169
        && prefixed.length() == 2L && prefixed.get(0L) == (byte)195 && prefixed.get(1L) == (byte)169
        && malformedAlias);
    boolean unrepresentable = false;
    match (render(document, "text", "US-ASCII", "<xsl:text>é</xsl:text>")) {
        Ok(bytes) {} Err(error) { unrepresentable = true; }
    }
    println(unrepresentable);
    boolean unsupportedEmpty = false;
    match (render(document, "p:text", "UTF-8", "")) {
        Ok(bytes) {} Err(error) { unsupportedEmpty = true; }
    }
    println(unsupportedEmpty);
    boolean unsupportedNonempty = false;
    match (render(document, "p:text", "UTF-8", "<xsl:text>value</xsl:text>")) {
        Ok(bytes) {} Err(error) { unsupportedNonempty = true; }
    }
    println(unsupportedNonempty);
    ByteBuffer shifted = render(document, "text", "ISO-2022-JP", "<xsl:text>あ</xsl:text>")?;
    println(shifted.length() == 8L && shifted.get(0L) == (byte)27 &&
        shifted.get(1L) == (byte)36 && shifted.get(2L) == (byte)66 &&
        shifted.get(3L) == (byte)36 && shifted.get(4L) == (byte)34 &&
        shifted.get(5L) == (byte)27 && shifted.get(6L) == (byte)40 &&
        shifted.get(7L) == (byte)66);
    ByteBuffer escaped = render(document, "xml", "US-ASCII", "<out>é</out>")?;
    Text ascii = new Text("US-ASCII");
    Text xml = Charset.decode(escaped, ascii)?;
    XmlDocument result = XmlDocument.parse(xml)?;
    Text path = new Text("/out/text()");
    List<Text> values = result.queryText(path)?;
    borrow Text value = values.getRef(0);
    println(value.equalsString("é"));
    ByteBuffer empty = render(document, "text", "UTF-16LE", "")?;
    println(empty.length() == 0L);
    return new Result<Unit, Text>.Ok(());
}
void main() { match (exercise()) { Ok(unit) {} Err(error) { error.print(); } } }
"#;
    let output = execute(source);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n");
    assert_memory_balanced(&output);
}
