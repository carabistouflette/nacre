(identifier) @variable
(type_identifier) @type
(primitive_type) @type.builtin
(parameter name: (identifier) @variable.parameter)
(class_declaration name: (identifier) @type)
(interface_declaration name: (identifier) @type)
(enum_declaration name: (identifier) @type)
(enum_variant name: (identifier) @constructor)
(new_expression variant: (identifier) @constructor)
(match_arm variant: (identifier) @constructor)
(match_binding name: (identifier) @variable.parameter)
(field_declaration name: (identifier) @property)
(member_expression property: (identifier) @property)
(call_expression function: (identifier) @function)
(call_expression function: (member_expression property: (identifier) @function))
 [("enum") ("match") ("class") ("interface") ("implements") ("public") ("private") ("static")
 ("readonly") ("borrow") ("mut") ("new") ("if") ("else") ("while") ("for")
 ("return") ("break") ("continue")] @keyword
(this_expression) @variable.special
(boolean_literal) @boolean
(integer_literal) @number
(string_literal) @string
(escape_sequence) @string.escape
(comment) @comment
["+" "-" "*" "/" "%" "=" "==" "!" "!=" "<" "<=" ">" ">=" "&&" "||"] @operator
["(" ")" "{" "}"] @punctuation.bracket
[";" "," "." ":"] @punctuation.delimiter
