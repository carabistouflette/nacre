const PREC = {
  OR: 1,
  AND: 2,
  EQUALITY: 3,
  COMPARISON: 4,
  ADDITIVE: 5,
  MULTIPLICATIVE: 6,
  UNARY: 7,
  POSTFIX: 8,
};

const commaSep1 = (rule) => seq(rule, repeat(seq(',', rule)));

module.exports = grammar({
  name: 'nacre',

  extras: ($) => [/\s/, $.comment],
  word: ($) => $.identifier,
  inline: ($) => [
    $._return_type, $.type_identifier, $.method_modifiers, $.top_level_modifiers,
    $.interface_method_modifiers,
  ],
  conflicts: ($) => [[$.generic_type, $._expression], [$.borrowed_type, $._expression]],

  rules: {
    source_file: ($) => repeat($._top_level_declaration),

    _top_level_declaration: ($) => choice(
      seq(optional('public'), $.class_declaration),
      seq(optional('public'), $.interface_declaration),
      seq(optional('public'), $.enum_declaration),
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

    top_level_modifiers: ($) => choice(
      $.visibility_modifier,
      'static',
      seq($.visibility_modifier, 'static'),
      seq('static', $.visibility_modifier),
    ),

    method_modifiers: ($) => choice(
      $.visibility_modifier,
      'static',
      'readonly',
      seq($.visibility_modifier, 'static'),
      seq('static', $.visibility_modifier),
      seq($.visibility_modifier, 'readonly'),
      seq('readonly', $.visibility_modifier),
    ),

    interface_method_modifiers: ($) => choice(
      'public',
      'readonly',
      seq('public', 'readonly'),
      seq('readonly', 'public'),
    ),

    visibility_modifier: ($) => choice('public', 'private'),

    type_parameters: ($) => seq(
      '<',
      commaSep1($.type_identifier),
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
      $.type_identifier,
      $.generic_type,
      $.borrowed_type,
    ),

    _return_type: ($) => choice($._type, alias('void', $.primitive_type)),

    _reference_type: ($) => choice($.type_identifier, $.generic_type),
    interface_list: ($) => commaSep1($._reference_type),

    primitive_type: ($) => choice('int', 'boolean', 'String'),

    type_identifier: ($) => alias($.identifier, $.type_identifier),

    generic_type: ($) => seq(
      field('name', $.type_identifier),
      field('type_arguments', $.type_arguments),
    ),

    borrowed_type: ($) => seq(
      'borrow',
      optional('mut'),
      field('name', $.type_identifier),
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
      field('variant', $.identifier),
      optional(field('bindings', $.match_bindings)),
      field('body', $.block),
    ),

    match_bindings: ($) => seq(
      '(',
      optional(commaSep1($.match_binding)),
      ')',
    ),

    match_binding: ($) => field('name', $.identifier),

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
      $.borrow_expression,
      $.call_expression,
      $.member_expression,
      $.new_expression,
      $.parenthesized_expression,
      $.this_expression,
      $.identifier,
      $.integer_literal,
      $.boolean_literal,
      $.string_literal,
    ),

    binary_expression: ($) => choice(
      prec.left(PREC.OR, seq(field('left', $._expression), field('operator', '||'), field('right', $._expression))),
      prec.left(PREC.AND, seq(field('left', $._expression), field('operator', '&&'), field('right', $._expression))),
      prec.left(PREC.EQUALITY, seq(field('left', $._expression), field('operator', choice('==', '!=')), field('right', $._expression))),
      prec.left(PREC.COMPARISON, seq(field('left', $._expression), field('operator', choice('<', '<=', '>', '>=')), field('right', $._expression))),
      prec.left(PREC.ADDITIVE, seq(field('left', $._expression), field('operator', choice('+', '-')), field('right', $._expression))),
      prec.left(PREC.MULTIPLICATIVE, seq(field('left', $._expression), field('operator', choice('*', '/', '%')), field('right', $._expression))),
    ),

    unary_expression: ($) => prec(PREC.UNARY, seq(
      field('operator', choice('+', '-', '!')),
      field('argument', $._expression),
    )),

    borrow_expression: ($) => prec(PREC.UNARY, seq(
      'borrow',
      optional('mut'),
      field('argument', $._expression),
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

    new_expression: ($) => seq(
      'new',
      field('type', choice($.type_identifier, $.generic_type)),
      optional(seq('.', field('variant', $.identifier))),
      field('arguments', $.argument_list),
    ),

    argument_list: ($) => seq(
      '(',
      optional(commaSep1($._expression)),
      ')',
    ),

    parenthesized_expression: ($) => seq('(', $._expression, ')'),

    this_expression: ($) => 'this',

    integer_literal: ($) => token(/[0-9]+/),

    boolean_literal: ($) => choice('true', 'false'),

    string_literal: ($) => seq(
      '"',
      repeat(choice(
        token.immediate(/[^"\\\n\r]+/),
        $.escape_sequence,
      )),
      token.immediate('"'),
    ),

    escape_sequence: ($) => token.immediate(seq('\\', /[nrt0"\\]/)),

    identifier: ($) => /[A-Za-z_][A-Za-z0-9_]*/,

    comment: ($) => token(choice(
      seq('//', /[^\n]*/),
      seq('/*', repeat(choice(/[^*]/, /\*+[^*\/]/)), /\*+/, '/'),
    )),
  },
});
