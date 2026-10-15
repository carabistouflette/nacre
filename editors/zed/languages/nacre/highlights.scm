(identifier) @variable
(type_identifier) @type
(namespace_identifier) @namespace
(primitive_type) @type.builtin
(numeric_type) @type.builtin
((type_identifier) @type.builtin
 (#match? @type.builtin "^(Array|Arrays|ByteBuffer|Channel|Command|Comparator|Console|Datagram|Deque|Directory|Env|ExitStatus|File|FileInfo|Files|Guard|List|Map|Math|Mutex|Option|Pair|Path|Process|ProcessOutput|Random|Receiver|Regex|RegexMatch|Result|SendError|Sender|Set|SocketAddress|Task|TcpListener|TcpStream|Text|Thread|Time|TlsConfig|TlsStream|UdpSocket|Uri|WebSocket|WebSocketMessage)$"))
(parameter name: (identifier) @variable.parameter)
(class_declaration name: (identifier) @type)
(interface_declaration name: (identifier) @type)
(enum_declaration name: (identifier) @type)
(enum_variant name: (identifier) @constructor)
(new_expression variant: (identifier) @constructor)
(match_arm pattern: (variant_pattern variant: (qualified_identifier) @constructor))
(record_pattern type: (qualified_identifier) @type)
(field_pattern name: (identifier) @property)
(match_binding name: (qualified_identifier) @variable.parameter)
(for_statement name: (identifier) @variable.parameter)
(for_statement key: (identifier) @variable.parameter)
(for_statement value: (identifier) @variable.parameter)
(field_declaration name: (identifier) @property)
(member_expression property: (identifier) @property)
(call_expression function: (identifier) @function)
(call_expression function: (member_expression property: (identifier) @function))
(generic_static_call_expression function: (identifier) @function)
["enum" "match" "class" "interface" "implements" "public" "private" "static"
 "readonly" "borrow" "mut" "new" "if" "else" "while" "for"
 "return" "break" "continue" "module" "import"] @keyword
(this_expression) @variable.special
(boolean_literal) @boolean
(integer_literal) @number
(long_literal) @number
(double_literal) @number
(unit_literal) @constant.builtin
(string_literal) @string
(character_literal) @string
(escape_sequence) @string.escape
(character_escape_sequence) @string.escape
(comment) @comment
["+" "-" "*" "/" "%" "=" "==" "!" "!=" "<" "<=" ">" ">=" "&&" "||" "=>"
 "|" "^" "&" "~" "<<" "?"] @operator
["(" ")" "{" "}"] @punctuation.bracket
[";" "," "." ":"] @punctuation.delimiter
