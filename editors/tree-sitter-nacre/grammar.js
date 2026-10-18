const PREC = {
  OR: 1,
  AND: 2,
  BITWISE_OR: 3,
  BITWISE_XOR: 4,
  BITWISE_AND: 5,
  EQUALITY: 6,
  COMPARISON: 7,
  SHIFT: 8,
  ADDITIVE: 9,
  MULTIPLICATIVE: 10,
  UNARY: 11,
  POSTFIX: 12,
};

const commaSep1 = (rule) => seq(rule, repeat(seq(',', rule)));
const commaSep1Trailing = (rule) => seq(rule, repeat(seq(',', rule)), optional(','));

module.exports = grammar({
  name: 'nacre',
  reserved: {
    global: ($) => [$._unsupported_keyword],
  },

  extras: ($) => [/\s/, $.comment],
  word: ($) => $.identifier,
  inline: ($) => [
    $._return_type, $.type_identifier, $.method_modifiers, $.top_level_modifiers,
    $.interface_method_modifiers, $.trait_method_modifiers, $.impl_method_modifiers,
  ],
  conflicts: ($) => [
    [$.qualified_type_identifier, $._expression],
    [$._type_identifier, $._expression],
    [$.record_pattern, $.match_binding],
    [$.field_declaration, $._method_function_declaration],
    [$.field_declaration, $.constructor_declaration, $._method_function_declaration],
  ],

  rules: {
    source_file: ($) => seq(
      optional($.module_declaration),
      repeat($.import_declaration),
      repeat($._top_level_declaration),
    ),

    module_declaration: ($) => seq(
      'module',
      field('name', $.qualified_identifier),
      ';',
    ),

    import_declaration: ($) => seq(
      'import',
      field('path', $.qualified_identifier),
      optional($.wildcard_import),
      ';',
    ),

    qualified_identifier: ($) => prec.right(seq(
      alias($.identifier, $.namespace_identifier),
      repeat(seq('.', alias($.identifier, $.namespace_identifier))),
    )),
    wildcard_import: ($) => token(prec(1, /\.\s*\*/)),

    _top_level_declaration: ($) => choice(
      seq(optional($.visibility_modifier), $.class_declaration),
      seq(optional($.visibility_modifier), $.interface_declaration),
      seq(optional($.visibility_modifier), $.enum_declaration),
      seq(optional($.visibility_modifier), $.trait_declaration),
      seq(optional($.visibility_modifier), $.impl_declaration),
      alias($._top_level_function_declaration, $.function_declaration),
    ),

    class_declaration: ($) => seq(
      'class',
      field('name', $.identifier),
      optional(field('type_parameters', $.type_parameters)),
      optional(seq('implements', field('interfaces', $.interface_list))),
      field('body', $.class_body),
    ),

    interface_declaration: ($) => seq(
      'interface',
      field('name', $.identifier),
      optional(field('type_parameters', $.type_parameters)),
      field('body', $.interface_body),
    ),

    trait_declaration: ($) => seq(
      'trait',
      field('name', $.identifier),
      field('body', $.trait_body),
    ),

    impl_declaration: ($) => seq(
      'impl',
      field('trait', $.type_identifier),
      'for',
      field('target', $.type_identifier),
      field('body', $.impl_body),
    ),

    enum_declaration: ($) => seq(
      'enum',
      field('name', $.identifier),
      optional(field('type_parameters', $.type_parameters)),
      field('body', $.enum_body),
    ),

    enum_body: ($) => seq(
      '{',
      repeat($.enum_variant),
      '}',
    ),

    enum_variant: ($) => seq(
      field('name', $.identifier),
      optional(field('fields', $.variant_fields)),
      ';',
    ),

    variant_fields: ($) => seq(
      '(',
      commaSep1($._type),
      ')',
    ),

    class_body: ($) => seq(
      '{',
      repeat(choice(
        $.field_declaration,
        $.constructor_declaration,
        alias($._method_function_declaration, $.function_declaration),
      )),
      '}',
    ),

    interface_body: ($) => seq(
      '{',
      repeat(alias($._interface_function_declaration, $.function_declaration)),
      '}',
    ),

    trait_body: ($) => seq(
      '{',
      repeat(alias($._trait_method_function_declaration, $.function_declaration)),
      '}',
    ),

    impl_body: ($) => seq(
      '{',
      repeat(alias($._impl_method_function_declaration, $.function_declaration)),
      '}',
    ),

    _trait_method_function_declaration: ($) => seq(
      optional(field('modifiers', $.trait_method_modifiers)),
      field('return_type', $._return_type),
      field('name', $.identifier),
      field('parameters', $.parameter_list),
      field('body', ';'),
    ),

    _impl_method_function_declaration: ($) => seq(
      optional(field('modifiers', $.impl_method_modifiers)),
      field('return_type', $._return_type),
      field('name', $.identifier),
      field('parameters', $.parameter_list),
      field('body', $.block),
    ),

    field_declaration: ($) => seq(
      optional(field('modifiers', $.visibility_modifier)),
      field('type', $._type),
      field('name', $.identifier),
      optional(seq('=', field('value', $._expression))),
      ';',
    ),

    constructor_declaration: ($) => seq(
      optional(field('modifiers', $.visibility_modifier)),
      field('name', $.identifier),
      field('parameters', $.parameter_list),
      field('body', $.block),
    ),

    _top_level_function_declaration: ($) => seq(
      optional(field('modifiers', $.top_level_modifiers)),
      field('return_type', $._return_type),
      field('name', $.identifier),
      field('parameters', $.parameter_list),
      field('body', $.block),
    ),

    _method_function_declaration: ($) => seq(
      optional(field('modifiers', $.method_modifiers)),
      field('return_type', $._return_type),
      field('name', $.identifier),
      field('parameters', $.parameter_list),
      field('body', $.block),
    ),

    _interface_function_declaration: ($) => seq(
      optional(field('modifiers', $.interface_method_modifiers)),
      field('return_type', $._return_type),
      field('name', $.identifier),
      field('parameters', $.parameter_list),
      field('body', ';'),
    ),

    top_level_modifiers: ($) => repeat1(choice(
      $.visibility_modifier,
      'static',
      'async',
    )),

    method_modifiers: ($) => repeat1(choice(
      $.visibility_modifier,
      'static',
      'readonly',
      'async',
    )),

    trait_method_modifiers: ($) => repeat1(choice(
      $.visibility_modifier,
      'readonly',
    )),

    impl_method_modifiers: ($) => repeat1(choice(
      $.visibility_modifier,
      'readonly',
      'async',
    )),

    interface_method_modifiers: ($) => choice(
      'public',
      'readonly',
      seq('public', 'readonly'),
      seq('readonly', 'public'),
    ),

    visibility_modifier: ($) => choice('public', 'private'),

    type_parameters: ($) => seq(
      '<',
      commaSep1(seq($.type_identifier, optional(seq(':', $.type_identifier)))),
      '>',
    ),

    parameter_list: ($) => seq(
      '(',
      optional(commaSep1($.parameter)),
      ')',
    ),

    parameter: ($) => seq(
      field('type', $._type),
      field('name', $.identifier),
    ),

    _type: ($) => choice(
      $.primitive_type,
      $._type_identifier,
      $.generic_type,
      $.borrowed_type,
      $.tuple_type,
    ),

    _return_type: ($) => choice($._type, alias('void', $.primitive_type)),

    _reference_type: ($) => choice($._type_identifier, $.generic_type),
    interface_list: ($) => commaSep1($._reference_type),

    primitive_type: ($) => choice('byte', 'char', 'int', 'long', 'double', 'Unit', 'boolean', 'String'),

    numeric_type: ($) => choice('byte', 'char', 'int', 'long', 'double'),

    type_identifier: ($) => alias($.identifier, $.type_identifier),

    qualified_type_identifier: ($) => prec.right(seq(
      repeat1(seq(alias($.identifier, $.namespace_identifier), '.')),
      $.type_identifier,
    )),

    _type_identifier: ($) => choice($.type_identifier, $.qualified_type_identifier),

    generic_type: ($) => seq(
      field('name', $._type_identifier),
      field('type_arguments', $.type_arguments),
    ),

    tuple_type: ($) => seq('(', $._type, ',', $._type, ')'),

    borrowed_type: ($) => seq(
      'borrow',
      optional('mut'),
      field('name', $._type_identifier),
      optional(field('type_arguments', $.type_arguments)),
    ),

    type_arguments: ($) => seq(
      '<',
      commaSep1($._type),
      '>',
    ),

    block: ($) => seq('{', repeat($._statement), '}'),

    _statement: ($) => choice(
      $.variable_declaration,
      $.if_statement,
      $.while_statement,
      $.for_statement,
      $.match_statement,
      $.return_statement,
      $.break_statement,
      $.continue_statement,
      $.assignment_statement,
      $.expression_statement,
      $.block,
    ),

    match_statement: ($) => seq(
      'match',
      '(',
      field('value', $._expression),
      ')',
      '{',
      repeat($.match_arm),
      '}',
    ),

    match_arm: ($) => seq(
      field('pattern', $._match_pattern),
      optional(seq('if', field('guard', $._expression))),
      optional('=>'),
      field('body', $.block),
    ),

    _match_pattern: ($) => choice(
      $.variant_pattern, $.record_pattern, $.tuple_pattern, $.match_binding,
    ),

    variant_pattern: ($) => seq(
      field('variant', $.qualified_identifier),
      field('bindings', $.match_bindings),
    ),

    record_pattern: ($) => seq(
      field('type', $.qualified_identifier),
      '{', optional(commaSep1Trailing($.field_pattern)), '}',
    ),

    field_pattern: ($) => seq(
      field('name', $.identifier), ':', field('pattern', $._match_pattern),
    ),

    tuple_pattern: ($) => seq('(', $._match_pattern, ',', $._match_pattern, ')'),

    match_bindings: ($) => seq(
      '(',
      optional(commaSep1Trailing($._match_pattern)),
      ')',
    ),

    match_binding: ($) => field('name', $.qualified_identifier),

    variable_declaration: ($) => prec(1, seq(
      field('type', $._type),
      field('name', $.identifier),
      optional(seq('=', field('value', $._expression))),
      ';',
    )),

    if_statement: ($) => seq(
      'if',
      '(',
      field('condition', $._expression),
      ')',
      field('consequence', $.block),
      optional(seq(
        'else',
        field('alternative', choice($.block, $.if_statement)),
      )),
    ),

    while_statement: ($) => seq(
      'while',
      '(',
      field('condition', $._expression),
      ')',
      field('body', $.block),
    ),

    for_statement: ($) => seq(
      'for',
      '(',
      choice(
        seq(
          field('type', $._type),
          field('name', $.identifier),
        ),
        seq(
          field('key_type', $._type),
          field('key', $.identifier),
          ',',
          field('value_type', $._type),
          field('value', $.identifier),
        ),
      ),
      ':',
      field('collection', $._expression),
      ')',
      field('body', $.block),
    ),

    return_statement: ($) => seq(
      'return',
      optional(field('value', $._expression)),
      ';',
    ),

    break_statement: ($) => seq('break', ';'),

    continue_statement: ($) => seq('continue', ';'),

    assignment_statement: ($) => seq(
      field('left', choice($.identifier, $.member_expression)),
      '=',
      field('right', $._expression),
      ';',
    ),

    expression_statement: ($) => seq(field('expression', $._expression), ';'),

    _expression: ($) => choice(
      $.binary_expression,
      $.unary_expression,
      $.numeric_cast_expression,
      $.await_expression,
      $.borrow_expression,
      $.generic_static_call_expression,
      $.result_propagation_expression,
      $.call_expression,
      $.member_expression,
      $.new_expression,
      $.unit_literal,
      $.parenthesized_expression,
      $.tuple_expression,
      $.this_expression,
      $.identifier,
      $.integer_literal,
      $.long_literal,
      $.double_literal,
      $.boolean_literal,
      $.character_literal,
      $.string_literal,
    ),

    binary_expression: ($) => choice(
      prec.left(PREC.OR, seq(field('left', $._expression), field('operator', '||'), field('right', $._expression))),
      prec.left(PREC.AND, seq(field('left', $._expression), field('operator', '&&'), field('right', $._expression))),
      prec.left(PREC.BITWISE_OR, seq(field('left', $._expression), field('operator', '|'), field('right', $._expression))),
      prec.left(PREC.BITWISE_XOR, seq(field('left', $._expression), field('operator', '^'), field('right', $._expression))),
      prec.left(PREC.BITWISE_AND, seq(field('left', $._expression), field('operator', '&'), field('right', $._expression))),
      prec.left(PREC.EQUALITY, seq(field('left', $._expression), field('operator', choice('==', '!=')), field('right', $._expression))),
      prec.left(PREC.COMPARISON, seq(field('left', $._expression), field('operator', choice('<', '<=', '>', '>=')), field('right', $._expression))),
      prec.left(PREC.SHIFT, seq(field('left', $._expression), field('operator', choice('<<', $.shift_right_operator)), field('right', $._expression))),
      prec.left(PREC.ADDITIVE, seq(field('left', $._expression), field('operator', choice('+', '-')), field('right', $._expression))),
      prec.left(PREC.MULTIPLICATIVE, seq(field('left', $._expression), field('operator', choice('*', '/', '%')), field('right', $._expression))),
    ),

    unary_expression: ($) => prec(PREC.UNARY, seq(
      field('operator', choice('+', '-', '!', '~')),
      field('argument', $._expression),
    )),

    await_expression: ($) => prec(PREC.UNARY, seq(
      'await',
      field('argument', $._expression),
    )),

    numeric_cast_expression: ($) => prec(PREC.UNARY, seq(
      '(',
      field('type', $.numeric_type),
      ')',
      field('argument', $._expression),
    )),

    borrow_expression: ($) => prec(PREC.UNARY, seq(
      'borrow',
      optional('mut'),
      field('argument', $._expression),
    )),
    result_propagation_expression: ($) => prec.left(PREC.POSTFIX, seq(
      field('expression', $._expression),
      '?',
    )),

    member_expression: ($) => prec.left(PREC.POSTFIX, seq(
      field('object', $._expression),
      '.',
      field('property', $.identifier),
    )),

    call_expression: ($) => prec.left(PREC.POSTFIX, seq(
      field('function', choice($.identifier, $.member_expression)),
      field('arguments', $.argument_list),
    )),
    generic_static_call_expression: ($) => prec.left(PREC.POSTFIX, seq(
      field('class', $.generic_type),
      '.',
      field('function', $.identifier),
      field('arguments', $.argument_list),
    )),

    new_expression: ($) => seq(
      'new',
      choice(
        seq(
          field('type', $._type_identifier),
          field('arguments', $.argument_list),
        ),
        seq(
          field('type', $.generic_type),
          optional(seq('.', field('variant', $.identifier))),
          field('arguments', $.argument_list),
        ),
      ),
    ),

    argument_list: ($) => seq(
      '(',
      optional(commaSep1($._expression)),
      ')',
    ),

    unit_literal: ($) => seq('(', ')'),

    parenthesized_expression: ($) => seq('(', $._expression, ')'),

    tuple_expression: ($) => seq('(', $._expression, ',', $._expression, ')'),

    this_expression: ($) => 'this',

    shift_right_operator: ($) => token('>>'),

    integer_literal: ($) => token(/[0-9]+/),

    long_literal: ($) => token(prec(1, /[0-9]+L/)),

    double_literal: ($) => token(prec(1, /(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)[eE][+-]?[0-9]+|[0-9]+\.[0-9]*|\.[0-9]+/)),

    boolean_literal: ($) => choice('true', 'false'),

    character_literal: ($) => seq(
      "'",
      choice(token.immediate(/[^'\\\n\r]/), $.character_escape_sequence),
      token.immediate("'"),
    ),

    character_escape_sequence: ($) => token.immediate(seq('\\', choice(/[nrt0"'\\]/, /u\{[0-9A-Fa-f]+\}/))),

    string_literal: ($) => seq(
      '"',
      repeat(choice(
        token.immediate(/[^"\\\n\r]+/),
        $.escape_sequence,
      )),
      token.immediate('"'),
    ),

    escape_sequence: ($) => token.immediate(seq('\\', /[nrt0"'\\]/)),

    _unsupported_keyword: ($) => token(choice(
      'extends', 'null', 'super', 'package', 'final',
      'throw', 'try', 'catch', 'switch', 'float',
    )),

    identifier: ($) => /[A-Za-z_][A-Za-z0-9_]*/,

    comment: ($) => token(choice(
      seq('//', /[^\n]*/),
      seq('/*', repeat(choice(/[^*]/, /\*+[^*\/]/)), /\*+/, '/'),
    )),
  },
});
