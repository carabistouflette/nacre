use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use std::collections::HashSet;
pub(super) const TEXT_INLINE_CAPACITY: usize = 23;
pub(super) fn marker(method: M) -> &'static str {
    match method {
        M::ListNew => "@LIST_NEW@", M::ListAdd => "@LIST_ADD@",
        M::ListSize => "@LIST_SIZE@", M::ListCapacity => "@LIST_CAPACITY@",
        M::ListIsEmpty => "@LIST_IS_EMPTY@", M::ListGet => "@LIST_GET@",
        M::ListGetRef => "@LIST_GET_REF@", M::ListGetMut => "@LIST_GET_MUT@",
        M::ListSet => "@LIST_SET@", M::ListPop => "@LIST_POP@",
        M::ListRemove => "@LIST_REMOVE@", M::ListTryGet => "@LIST_TRY_GET@",
        M::ListTryPop => "@LIST_TRY_POP@", M::ListTryRemove => "@LIST_TRY_REMOVE@",
        M::ListClear => "@LIST_CLEAR@", M::ListReserve => "@LIST_RESERVE@",
        M::ArrayNew => "@ARRAY_NEW@", M::ArraySize => "@ARRAY_SIZE@",
        M::ArrayGet => "@ARRAY_GET@", M::ArrayGetRef => "@ARRAY_GET_REF@",
        M::ArrayGetMut => "@ARRAY_GET_MUT@", M::ArraySet => "@ARRAY_SET@",
        M::ArrayTryGet => "@ARRAY_TRY_GET@",
        M::ArraysBytes => "@ARRAYS_BYTES@", M::ArraysChars => "@ARRAYS_CHARS@",
        M::ArraysInts => "@ARRAYS_INTS@", M::ArraysLongs => "@ARRAYS_LONGS@",
        M::ArraysDoubles => "@ARRAYS_DOUBLES@", M::ArraysBooleans => "@ARRAYS_BOOLEANS@",
        M::ArraysUnits => "@ARRAYS_UNITS@",
        M::ByteBufferWithCapacity => "@BYTE_BUFFER_WITH_CAPACITY@",
        M::ByteBufferLength => "@BYTE_BUFFER_LENGTH@", M::ByteBufferCapacity => "@BYTE_BUFFER_CAPACITY@",
        M::ByteBufferGet => "@BYTE_BUFFER_GET@", M::ByteBufferSet => "@BYTE_BUFFER_SET@",
        M::ByteBufferAppend => "@BYTE_BUFFER_APPEND@",
        M::ByteBufferAppendBytes => "@BYTE_BUFFER_APPEND_BYTES@",
        M::ByteBufferClear => "@BYTE_BUFFER_CLEAR@", M::ByteBufferTruncate => "@BYTE_BUFFER_TRUNCATE@",
        M::EncodingHexEncode => "@ENCODING_HEX_ENCODE@",
        M::EncodingHexDecode => "@ENCODING_HEX_DECODE@",
        M::EncodingBase64Encode => "@ENCODING_BASE64_ENCODE@",
        M::EncodingBase64Decode => "@ENCODING_BASE64_DECODE@",
        M::EncodingBase64UrlEncode => "@ENCODING_BASE64_URL_ENCODE@",
        M::EncodingBase64UrlDecode => "@ENCODING_BASE64_URL_DECODE@",
        M::EncodingUtf8Encode => "@ENCODING_UTF8_ENCODE@",
        M::EncodingUtf8Decode => "@ENCODING_UTF8_DECODE@",
        M::UuidParse => "@UUID_PARSE_FN@", M::UuidFromBytes => "@UUID_FROM_BYTES_FN@",
        M::UuidFromLongs => "@UUID_FROM_LONGS_FN@", M::UuidRandom => "@UUID_RANDOM_FN@",
        M::UuidNameMd5 => "@UUID_NAME_MD5_FN@", M::UuidNameSha1 => "@UUID_NAME_SHA1_FN@",
        M::UuidClone => "@UUID_CLONE_FN@", M::UuidToText => "@UUID_TO_TEXT_FN@",
        M::UuidToBytes => "@UUID_TO_BYTES_FN@",
        M::UuidMostSignificantBits => "@UUID_MOST_SIGNIFICANT_BITS_FN@",
        M::UuidLeastSignificantBits => "@UUID_LEAST_SIGNIFICANT_BITS_FN@",
        M::UuidVersion => "@UUID_VERSION_FN@", M::UuidVariant => "@UUID_VARIANT_FN@",
        M::UuidEquals => "@UUID_EQUALS_FN@", M::UuidCompareTo => "@UUID_COMPARE_TO_FN@",
        M::ListSort => "@LIST_SORT@", M::ListSortBy => "@LIST_SORT_BY@",
        M::ListBinarySearch => "@LIST_BINARY_SEARCH@", M::ListBinarySearchBy => "@LIST_BINARY_SEARCH_BY@",
        M::DequeNew => "@DEQUE_NEW@", M::DequePushFront => "@DEQUE_PUSH_FRONT@",
        M::DequePushBack => "@DEQUE_PUSH_BACK@", M::DequePopFront => "@DEQUE_POP_FRONT@",
        M::DequePopBack => "@DEQUE_POP_BACK@", M::DequeTryPopFront => "@DEQUE_TRY_POP_FRONT@",
        M::DequeTryPopBack => "@DEQUE_TRY_POP_BACK@", M::DequeSize => "@DEQUE_SIZE@",
        M::DequeCapacity => "@DEQUE_CAPACITY@", M::DequeIsEmpty => "@DEQUE_IS_EMPTY@",
        M::DequeGet => "@DEQUE_GET@", M::DequeGetRef => "@DEQUE_GET_REF@",
        M::DequeGetMut => "@DEQUE_GET_MUT@", M::DequeSet => "@DEQUE_SET@",
        M::DequeTryGet => "@DEQUE_TRY_GET@", M::DequeClear => "@DEQUE_CLEAR@",
        M::DequeReserve => "@DEQUE_RESERVE@",
        M::CollectionDrain => "@COLLECTION_DRAIN@",
        M::SetNew => "@SET_NEW@", M::SetWithHasher => "@SET_WITH_HASHER@",
        M::SetAdd => "@SET_ADD@",
        M::SetContains => "@SET_CONTAINS@", M::SetSize => "@SET_SIZE@",
        M::SetIsEmpty => "@SET_IS_EMPTY@", M::SetRemove => "@SET_REMOVE@",
        M::SetClear => "@SET_CLEAR@",
        M::ListDrain => "@LIST_DRAIN@", M::ListCollect => "@LIST_COLLECT@",
        M::MapDrainKeys => "@MAP_DRAIN_KEYS@", M::MapDrainValues => "@MAP_DRAIN_VALUES@",
        M::OrderedMapNew => "@ORDERED_MAP_NEW@", M::OrderedMapPut => "@ORDERED_MAP_PUT@",
        M::OrderedMapTryRemove => "@ORDERED_MAP_TRY_REMOVE@",
        M::OrderedMapContains => "@ORDERED_MAP_CONTAINS@",
        M::OrderedMapTryGet => "@ORDERED_MAP_TRY_GET@",
        M::OrderedMapGetRef => "@ORDERED_MAP_GET_REF@",
        M::OrderedMapGetMut => "@ORDERED_MAP_GET_MUT@",
        M::OrderedMapKeyAt => "@ORDERED_MAP_KEY_AT@",
        M::OrderedMapValueAt => "@ORDERED_MAP_VALUE_AT@",
        M::OrderedMapValueAtMut => "@ORDERED_MAP_VALUE_AT_MUT@",
        M::OrderedMapDrainKeys => "@ORDERED_MAP_DRAIN_KEYS@",
        M::OrderedMapDrainValues => "@ORDERED_MAP_DRAIN_VALUES@",
        M::OrderedMapSize => "@ORDERED_MAP_SIZE@",
        M::OrderedMapIsEmpty => "@ORDERED_MAP_IS_EMPTY@",
        M::OrderedMapClear => "@ORDERED_MAP_CLEAR@",
        M::OrderedMapFloorIndex => "@ORDERED_MAP_FLOOR_INDEX@",
        M::OrderedMapCeilingIndex => "@ORDERED_MAP_CEILING_INDEX@",
        M::OrderedMapLowerIndex => "@ORDERED_MAP_LOWER_INDEX@",
        M::OrderedMapHigherIndex => "@ORDERED_MAP_HIGHER_INDEX@",
        M::OrderedSetNew => "@ORDERED_SET_NEW@", M::OrderedSetAdd => "@ORDERED_SET_ADD@",
        M::OrderedSetContains => "@ORDERED_SET_CONTAINS@",
        M::OrderedSetRemove => "@ORDERED_SET_REMOVE@",
        M::OrderedSetSize => "@ORDERED_SET_SIZE@",
        M::OrderedSetIsEmpty => "@ORDERED_SET_IS_EMPTY@",
        M::OrderedSetClear => "@ORDERED_SET_CLEAR@",
        M::OrderedSetKeyAt => "@ORDERED_SET_KEY_AT@",
        M::OrderedSetDrain => "@ORDERED_SET_DRAIN@",
        M::PriorityQueueNew => "@PRIORITY_QUEUE_NEW@",
        M::PriorityQueueAdd => "@PRIORITY_QUEUE_ADD@",
        M::PriorityQueuePop => "@PRIORITY_QUEUE_POP@",
        M::PriorityQueueTryPop => "@PRIORITY_QUEUE_TRY_POP@",
        M::PriorityQueuePeek => "@PRIORITY_QUEUE_PEEK@",
        M::PriorityQueueGetRef => "@PRIORITY_QUEUE_GET_REF@",
        M::PriorityQueueTryPeek => "@PRIORITY_QUEUE_TRY_PEEK@",
        M::PriorityQueueSize => "@PRIORITY_QUEUE_SIZE@",
        M::PriorityQueueIsEmpty => "@PRIORITY_QUEUE_IS_EMPTY@",
        M::PriorityQueueClear => "@PRIORITY_QUEUE_CLEAR@",
        M::PriorityQueueValueAt => "@PRIORITY_QUEUE_VALUE_AT@",
        M::PriorityQueueDrain => "@PRIORITY_QUEUE_DRAIN@",
        M::BitSetNew => "@BIT_SET_NEW@", M::BitSetGet => "@BIT_SET_GET@",
        M::BitSetSet => "@BIT_SET_SET@", M::BitSetClear => "@BIT_SET_CLEAR@",
        M::BitSetFlip => "@BIT_SET_FLIP@", M::BitSetSetRange => "@BIT_SET_SET_RANGE@",
        M::BitSetClearRange => "@BIT_SET_CLEAR_RANGE@", M::BitSetFlipRange => "@BIT_SET_FLIP_RANGE@",
        M::BitSetNextSetBit => "@BIT_SET_NEXT_SET_BIT@",
        M::BitSetCardinality => "@BIT_SET_CARDINALITY@", M::BitSetLength => "@BIT_SET_LENGTH@",
        M::BitSetIsEmpty => "@BIT_SET_IS_EMPTY@", M::BitSetClone => "@BIT_SET_CLONE@",
        M::BitSetAnd => "@BIT_SET_AND@", M::BitSetOr => "@BIT_SET_OR@",
        M::BitSetXor => "@BIT_SET_XOR@", M::BitSetAndNot => "@BIT_SET_AND_NOT@",
        M::ListIteratorNext => "@LIST_ITERATOR_NEXT@",
        M::MapIteratorNew => "@MAP_ITERATOR_NEW@", M::MapIteratorNext => "@MAP_ITERATOR_NEXT@",
        M::FilterIteratorNew => "@FILTER_ITERATOR_NEW@", M::FilterIteratorNext => "@FILTER_ITERATOR_NEXT@",
        M::TakeIteratorNew => "@TAKE_ITERATOR_NEW@", M::TakeIteratorNext => "@TAKE_ITERATOR_NEXT@",
        M::SkipIteratorNew => "@SKIP_ITERATOR_NEW@", M::SkipIteratorNext => "@SKIP_ITERATOR_NEXT@",
        M::IteratorsTake => "@ITERATORS_TAKE@", M::IteratorsSkip => "@ITERATORS_SKIP@",
        M::PathFromText => "@PATH_FROM_TEXT@", M::PathClone => "@PATH_CLONE@",
        M::PathJoin => "@PATH_JOIN@", M::PathIsAbsolute => "@PATH_IS_ABSOLUTE@",
        M::PathAsText => "@PATH_AS_TEXT@",
        M::FileOpenRead => "@FILE_OPEN_READ@", M::FileCreate => "@FILE_CREATE@",
        M::FileOpenAppend => "@FILE_OPEN_APPEND@", M::FileOpenReadWrite => "@FILE_OPEN_READ_WRITE@",
        M::FileReadSome => "@FILE_READ_SOME@", M::FileWriteSome => "@FILE_WRITE_SOME@",
        M::FileRead => "@FILE_READ@", M::FileWrite => "@FILE_WRITE@",
        M::FileWriteAll => "@FILE_WRITE_ALL@", M::FilePosition => "@FILE_POSITION@",
        M::FileSeek => "@FILE_SEEK@", M::FileTruncate => "@FILE_TRUNCATE@",
        M::FileReadAt => "@FILE_READ_AT@", M::FileWriteAt => "@FILE_WRITE_AT@",
        M::FileFlush => "@FILE_FLUSH@", M::FileClose => "@FILE_CLOSE@",
        M::DirectoryOpen => "@DIRECTORY_OPEN@", M::DirectoryNextName => "@DIRECTORY_NEXT_NAME@",
        M::DirectoryClose => "@DIRECTORY_CLOSE@",
        M::FileInfoSize => "@FILE_INFO_SIZE@", M::FileInfoIsFile => "@FILE_INFO_IS_FILE@",
        M::FileInfoIsDirectory => "@FILE_INFO_IS_DIRECTORY@", M::FileInfoIsSymlink => "@FILE_INFO_IS_SYMLINK@",
        M::MapNew => "@MAP_NEW@", M::MapWithHasher => "@MAP_WITH_HASHER@",
        M::MapContainsKey => "@MAP_CONTAINS_KEY@", M::MapSize => "@MAP_SIZE@",
        M::MapIsEmpty => "@MAP_IS_EMPTY@", M::MapPut => "@MAP_PUT@",
        M::MapTryRemove => "@MAP_TRY_REMOVE@", M::MapTryGet => "@MAP_TRY_GET@",
        M::MapGetRef => "@MAP_GET_REF@", M::MapGetMut => "@MAP_GET_MUT@",
        M::MapClear => "@MAP_CLEAR@",
        M::FileInfoModifiedMillis => "@FILE_INFO_MODIFIED_MILLIS@",
        M::FileInfoAccessedMillis => "@FILE_INFO_ACCESSED_MILLIS@",
        M::FileInfoChangedMillis => "@FILE_INFO_CHANGED_MILLIS@", M::FileInfoMode => "@FILE_INFO_MODE@",
        M::WatcherOpen => "@WATCHER_OPEN@", M::WatcherNext => "@WATCHER_NEXT@", M::WatcherClose => "@WATCHER_CLOSE@",
        M::WatchEventName => "@WATCH_EVENT_NAME@", M::WatchEventKind => "@WATCH_EVENT_KIND@",
        M::WatchEventCookie => "@WATCH_EVENT_COOKIE@", M::WatchEventIsDirectory => "@WATCH_EVENT_IS_DIRECTORY@",
        M::WatchEventOverflow => "@WATCH_EVENT_OVERFLOW@",
        M::DatabaseOpen => "@DATABASE_OPEN@", M::DatabasePrepare => "@DATABASE_PREPARE@",
        M::DatabaseChanges => "@DATABASE_CHANGES@",
        M::DatabaseLastInsertRowId => "@DATABASE_LAST_INSERT_ROW_ID@",
        M::DatabaseExecute => "@DATABASE_EXECUTE@", M::DatabaseBegin => "@DATABASE_BEGIN@",
        M::DatabaseCommit => "@DATABASE_COMMIT@", M::DatabaseRollback => "@DATABASE_ROLLBACK@",
        M::DatabaseSetBusyTimeoutMillis => "@DATABASE_SET_BUSY_TIMEOUT_MILLIS@",
        M::DatabaseClose => "@DATABASE_CLOSE@",
        M::SqlStatementBindNull => "@SQL_STATEMENT_BIND_NULL@",
        M::SqlStatementBindLong => "@SQL_STATEMENT_BIND_LONG@",
        M::SqlStatementBindDouble => "@SQL_STATEMENT_BIND_DOUBLE@",
        M::SqlStatementBindText => "@SQL_STATEMENT_BIND_TEXT@",
        M::SqlStatementBindBlob => "@SQL_STATEMENT_BIND_BLOB@",
        M::SqlStatementStep => "@SQL_STATEMENT_STEP@",
        M::SqlStatementReset => "@SQL_STATEMENT_RESET@",
        M::SqlStatementClearBindings => "@SQL_STATEMENT_CLEAR_BINDINGS@",
        M::SqlStatementColumnCount => "@SQL_STATEMENT_COLUMN_COUNT@",
        M::SqlStatementColumnName => "@SQL_STATEMENT_COLUMN_NAME@",
        M::SqlStatementColumnType => "@SQL_STATEMENT_COLUMN_TYPE@",
        M::SqlStatementColumnLong => "@SQL_STATEMENT_COLUMN_LONG@",
        M::SqlStatementColumnDouble => "@SQL_STATEMENT_COLUMN_DOUBLE@",
        M::SqlStatementColumnText => "@SQL_STATEMENT_COLUMN_TEXT@",
        M::SqlStatementColumnBlob => "@SQL_STATEMENT_COLUMN_BLOB@",
        M::SqlStatementClose => "@SQL_STATEMENT_CLOSE@",
        M::BinaryReaderNew => "@BINARY_READER_NEW@",
        M::BinaryReaderFromSource => "@BINARY_READER_FROM_SOURCE@",
        M::BinaryReaderPosition => "@BINARY_READER_POSITION@",
        M::BinaryReaderRemaining => "@BINARY_READER_REMAINING@",
        M::BinaryReaderSeek => "@BINARY_READER_SEEK@",
        M::BinaryReaderReadByte => "@BINARY_READER_READ_BYTE@",
        M::BinaryReaderReadInt16 => "@BINARY_READER_READ_INT16@",
        M::BinaryReaderReadInt32 => "@BINARY_READER_READ_INT32@",
        M::BinaryReaderReadInt64 => "@BINARY_READER_READ_INT64@",
        M::BinaryReaderReadDouble => "@BINARY_READER_READ_DOUBLE@",
        M::BinaryReaderReadFloat32 => "@BINARY_READER_READ_FLOAT32@",
        M::BinaryReaderReadBytes => "@BINARY_READER_READ_BYTES@",
        M::BinaryReaderReadUtf8 => "@BINARY_READER_READ_UTF8@",
        M::BinaryWriterNew => "@BINARY_WRITER_NEW@",
        M::BinaryWriterToSink => "@BINARY_WRITER_TO_SINK@",
        M::BinaryWriterPosition => "@BINARY_WRITER_POSITION@",
        M::BinaryWriterPendingOffset => "@BINARY_WRITER_PENDING_OFFSET@",
        M::BinaryWriterFlush => "@BINARY_WRITER_FLUSH@",
        M::BinaryWriterWriteByte => "@BINARY_WRITER_WRITE_BYTE@",
        M::BinaryWriterWriteInt16 => "@BINARY_WRITER_WRITE_INT16@",
        M::BinaryWriterWriteInt32 => "@BINARY_WRITER_WRITE_INT32@",
        M::BinaryWriterWriteInt64 => "@BINARY_WRITER_WRITE_INT64@",
        M::BinaryWriterWriteDouble => "@BINARY_WRITER_WRITE_DOUBLE@",
        M::BinaryWriterWriteFloat32 => "@BINARY_WRITER_WRITE_FLOAT32@",
        M::BinaryWriterWriteBytes => "@BINARY_WRITER_WRITE_BYTES@",
        M::BinaryWriterWriteUtf8 => "@BINARY_WRITER_WRITE_UTF8@",
        M::BinaryWriterBytes => "@BINARY_WRITER_BYTES@",
        M::BinaryWriterFinish => "@BINARY_WRITER_FINISH@",
        M::TextNew => "@TEXT_NEW@",
        M::TextAppend => "@TEXT_APPEND@", M::TextAppendInt => "@TEXT_APPEND_INT@",
        M::TextAppendByte => "@TEXT_APPEND_BYTE@", M::TextAppendChar => "@TEXT_APPEND_CHAR@",
        M::TextAppendLong => "@TEXT_APPEND_LONG@", M::TextAppendDouble => "@TEXT_APPEND_DOUBLE@",
        M::TextAppendBoolean => "@TEXT_APPEND_BOOLEAN@", M::TextAppendText => "@TEXT_APPEND_TEXT@",
        M::TextByteLength => "@TEXT_BYTE_LENGTH@", M::TextByteAt => "@TEXT_BYTE_AT@",
        M::TextCapacity => "@TEXT_CAPACITY@", M::TextReserve => "@TEXT_RESERVE@",
        M::TextIsEmpty => "@TEXT_IS_EMPTY@", M::TextClear => "@TEXT_CLEAR@",
        M::TextPrint => "@TEXT_PRINT@", M::TextClone => "@TEXT_CLONE@",
        M::TextEquals => "@TEXT_EQUALS@", M::TextEqualsString => "@TEXT_EQUALS_STRING@",
        M::TextParseInt => "@TEXT_PARSE_INT@", M::TextSliceBytes => "@TEXT_SLICE_BYTES@",
        M::TextParseLong => "@TEXT_PARSE_LONG@", M::TextParseDouble => "@TEXT_PARSE_DOUBLE@",
        M::TextCharCount => "@TEXT_CHAR_COUNT@", M::TextCharAt => "@TEXT_CHAR_AT@",
        M::TextCompareTo => "@TEXT_COMPARE_TO@", M::TextIndexOf => "@TEXT_INDEX_OF@",
        M::TextStartsWith => "@TEXT_STARTS_WITH@", M::TextEndsWith => "@TEXT_ENDS_WITH@",
        M::TextSlice => "@TEXT_SLICE@", M::TextTrim => "@TEXT_TRIM@",
        M::TextReplace => "@TEXT_REPLACE@", M::TextSplit => "@TEXT_SPLIT@", M::TextJoin => "@TEXT_JOIN@",
        M::UnicodeToUpperCase => "@UNICODE_TO_UPPER_CASE@",
        M::UnicodeToLowerCase => "@UNICODE_TO_LOWER_CASE@",
        M::UnicodeCaseFold => "@UNICODE_CASE_FOLD@",
        M::UnicodeNormalize => "@UNICODE_NORMALIZE@",
        M::UnicodeGraphemeCount => "@UNICODE_GRAPHEME_COUNT@",
        M::UnicodeGraphemeAt => "@UNICODE_GRAPHEME_AT@",
        M::UnicodeCompareLocale => "@UNICODE_COMPARE_LOCALE@",
        M::CharacterIsLetter => "@CHARACTER_IS_LETTER@",
        M::CharacterIsDigit => "@CHARACTER_IS_DIGIT@",
        M::CharacterIsWhitespace => "@CHARACTER_IS_WHITESPACE@",
        M::CharacterIsUpperCase => "@CHARACTER_IS_UPPER_CASE@",
        M::CharacterIsLowerCase => "@CHARACTER_IS_LOWER_CASE@",
        M::CharacterToUpperCase => "@CHARACTER_TO_UPPER_CASE@",
        M::CharacterToLowerCase => "@CHARACTER_TO_LOWER_CASE@",
        M::CharacterCategory => "@CHARACTER_CATEGORY@",
        M::CharacterNumericValue => "@CHARACTER_NUMERIC_VALUE@",
        M::CharsetAvailable => "@CHARSET_AVAILABLE@",
        M::CharsetEncode => "@CHARSET_ENCODE@",
        M::CharsetDecode => "@CHARSET_DECODE@",
        M::LocaleAvailable => "@LOCALE_AVAILABLE@",
        M::LocaleFormatNumber => "@LOCALE_FORMAT_NUMBER@",
        M::LocaleFormatDecimal => "@LOCALE_FORMAT_DECIMAL@",
        M::LocaleParseNumber => "@LOCALE_PARSE_NUMBER@",
        M::LocaleParseDecimal => "@LOCALE_PARSE_DECIMAL@",
        M::LocaleFormatCurrency => "@LOCALE_FORMAT_CURRENCY@",
        M::LocaleFormatCurrencyDecimal => "@LOCALE_FORMAT_CURRENCY_DECIMAL@",
        M::RegexCompile => "@REGEX_COMPILE@", M::RegexMatches => "@REGEX_MATCHES@",
        M::RegexSearch => "@REGEX_SEARCH@",
        M::RegexMatchGroupCount => "@REGEX_MATCH_GROUP_COUNT@",
        M::RegexMatchHasGroup => "@REGEX_MATCH_HAS_GROUP@", M::RegexMatchGroup => "@REGEX_MATCH_GROUP@",
        M::RegexMatchStart => "@REGEX_MATCH_START@", M::RegexMatchEnd => "@REGEX_MATCH_END@",
        M::ConsoleReadLine => "@CONSOLE_READ_LINE@", M::ConsolePrint => "@CONSOLE_PRINT@",
        M::ConsolePrintText => "@CONSOLE_PRINT_TEXT@", M::FilesReadText => "@FILES_READ_TEXT@",
        M::FilesWriteText => "@FILES_WRITE_TEXT@", M::FilesWriteTextAtomic => "@FILES_WRITE_TEXT_ATOMIC@",
        M::FilesMetadata => "@FILES_METADATA@", M::FilesCreateDirectory => "@FILES_CREATE_DIRECTORY@",
        M::FilesRemoveFile => "@FILES_REMOVE_FILE@", M::FilesRemoveDirectory => "@FILES_REMOVE_DIRECTORY@",
        M::FilesRename => "@FILES_RENAME@", M::FilesCopy => "@FILES_COPY@",
        M::FilesCreateDirectories => "@FILES_CREATE_DIRECTORIES@", M::FilesReadLink => "@FILES_READ_LINK@",
        M::FilesSymlink => "@FILES_SYMLINK@", M::FilesChmod => "@FILES_CHMOD@",
        M::FilesList => "@FILES_LIST@", M::FilesCanonical => "@FILES_CANONICAL@",
        M::ArgsCount => "@ARGS_COUNT@", M::ArgsGet => "@ARGS_GET@",
        M::MathAbsInt => "@MATH_ABS_INT@", M::MathAbsLong => "@MATH_ABS_LONG@",
        M::MathAbsDouble => "@MATH_ABS_DOUBLE@", M::MathMinInt => "@MATH_MIN_INT@",
        M::MathMinLong => "@MATH_MIN_LONG@", M::MathMinDouble => "@MATH_MIN_DOUBLE@",
        M::MathMaxInt => "@MATH_MAX_INT@", M::MathMaxLong => "@MATH_MAX_LONG@",
        M::MathMaxDouble => "@MATH_MAX_DOUBLE@", M::MathSqrt => "@MATH_SQRT@",
        M::MathPow => "@MATH_POW@", M::MathLog => "@MATH_LOG@", M::MathExp => "@MATH_EXP@",
        M::MathSin => "@MATH_SIN@", M::MathCos => "@MATH_COS@", M::MathTan => "@MATH_TAN@",
        M::MathFloor => "@MATH_FLOOR@", M::MathCeil => "@MATH_CEIL@", M::MathRound => "@MATH_ROUND@",
        M::MathHypot => "@MATH_HYPOT@", M::MathIsNaN => "@MATH_IS_NAN@", M::MathIsFinite => "@MATH_IS_FINITE@",
        M::MathCbrt => "@MATH_CBRT@", M::MathLog10 => "@MATH_LOG10@",
        M::MathLog1p => "@MATH_LOG1P@", M::MathExpm1 => "@MATH_EXPM1@",
        M::MathAsin => "@MATH_ASIN@", M::MathAcos => "@MATH_ACOS@",
        M::MathAtan => "@MATH_ATAN@", M::MathAtan2 => "@MATH_ATAN2@",
        M::MathSinh => "@MATH_SINH@", M::MathCosh => "@MATH_COSH@",
        M::MathTanh => "@MATH_TANH@", M::MathToRadians => "@MATH_TO_RADIANS@",
        M::MathToDegrees => "@MATH_TO_DEGREES@", M::MathSignum => "@MATH_SIGNUM@",
        M::MathCopySign => "@MATH_COPY_SIGN@", M::MathNextUp => "@MATH_NEXT_UP@",
        M::MathNextDown => "@MATH_NEXT_DOWN@", M::MathUlp => "@MATH_ULP@",
        M::MathGetExponent => "@MATH_GET_EXPONENT@", M::MathScalb => "@MATH_SCALB@",
        M::MathIeeeRemainder => "@MATH_IEEE_REMAINDER@",
        M::MathFma => "@MATH_FMA@", M::MathNextAfter => "@MATH_NEXT_AFTER@",
        M::MathPi => "@MATH_PI@", M::MathE => "@MATH_E@",
        M::MathAddExactInt => "@MATH_ADD_EXACT_INT@", M::MathAddExactLong => "@MATH_ADD_EXACT_LONG@",
        M::MathSubtractExactInt => "@MATH_SUBTRACT_EXACT_INT@", M::MathSubtractExactLong => "@MATH_SUBTRACT_EXACT_LONG@",
        M::MathMultiplyExactInt => "@MATH_MULTIPLY_EXACT_INT@", M::MathMultiplyExactLong => "@MATH_MULTIPLY_EXACT_LONG@",
        M::MathNegateExactInt => "@MATH_NEGATE_EXACT_INT@", M::MathNegateExactLong => "@MATH_NEGATE_EXACT_LONG@",
        M::MathIncrementExactInt => "@MATH_INCREMENT_EXACT_INT@", M::MathIncrementExactLong => "@MATH_INCREMENT_EXACT_LONG@",
        M::MathDecrementExactInt => "@MATH_DECREMENT_EXACT_INT@", M::MathDecrementExactLong => "@MATH_DECREMENT_EXACT_LONG@",
        M::MathFloorDivInt => "@MATH_FLOOR_DIV_INT@", M::MathFloorDivLong => "@MATH_FLOOR_DIV_LONG@",
        M::MathFloorModInt => "@MATH_FLOOR_MOD_INT@", M::MathFloorModLong => "@MATH_FLOOR_MOD_LONG@",
        M::CryptoRandomBytes => "@CRYPTO_RANDOM_BYTES@",
        M::CryptoDigest => "@CRYPTO_DIGEST@",
        M::CryptoHmac => "@CRYPTO_HMAC@",
        M::CryptoPbkdf2 => "@CRYPTO_PBKDF2@",
        M::CryptoHkdf => "@CRYPTO_HKDF@",
        M::CryptoAesGcmEncrypt => "@CRYPTO_AES_GCM_ENCRYPT@",
        M::CryptoAesGcmDecrypt => "@CRYPTO_AES_GCM_DECRYPT@",
        M::CryptoConstantTimeEquals => "@CRYPTO_CONSTANT_TIME_EQUALS@",
        M::BigIntFromLong => "@BIGINT_FROM_LONG@", M::BigIntParse => "@BIGINT_PARSE@",
        M::BigIntClone => "@BIGINT_CLONE@", M::BigIntToText => "@BIGINT_TO_TEXT@",
        M::BigIntCompareTo => "@BIGINT_COMPARE_TO@", M::BigIntSignum => "@BIGINT_SIGNUM@",
        M::BigIntBitLength => "@BIGINT_BIT_LENGTH@", M::BigIntAdd => "@BIGINT_ADD@",
        M::BigIntSubtract => "@BIGINT_SUBTRACT@", M::BigIntMultiply => "@BIGINT_MULTIPLY@",
        M::BigIntDivide => "@BIGINT_DIVIDE@", M::BigIntRemainder => "@BIGINT_REMAINDER@",
        M::BigIntGcd => "@BIGINT_GCD@", M::BigIntMod => "@BIGINT_MOD@",
        M::BigIntPow => "@BIGINT_POW@", M::BigIntModPow => "@BIGINT_MOD_POW@",
        M::BigIntModInverse => "@BIGINT_MOD_INVERSE@", M::BigIntNegate => "@BIGINT_NEGATE@",
        M::BigIntAbs => "@BIGINT_ABS@", M::BigIntShiftLeft => "@BIGINT_SHIFT_LEFT@",
        M::BigIntShiftRight => "@BIGINT_SHIFT_RIGHT@", M::BigIntBitAnd => "@BIGINT_BIT_AND@",
        M::BigIntBitOr => "@BIGINT_BIT_OR@", M::BigIntBitXor => "@BIGINT_BIT_XOR@",
        M::BigIntBitNot => "@BIGINT_BIT_NOT@", M::BigIntTestBit => "@BIGINT_TEST_BIT@",
        M::BigIntToLong => "@BIGINT_TO_LONG@",
        M::BigDecimalParse => "@BIGDECIMAL_PARSE@", M::BigDecimalFromLong => "@BIGDECIMAL_FROM_LONG@",
        M::BigDecimalClone => "@BIGDECIMAL_CLONE@", M::BigDecimalToText => "@BIGDECIMAL_TO_TEXT@",
        M::BigDecimalScale => "@BIGDECIMAL_SCALE@", M::BigDecimalCompareTo => "@BIGDECIMAL_COMPARE_TO@",
        M::BigDecimalSignum => "@BIGDECIMAL_SIGNUM@", M::BigDecimalAdd => "@BIGDECIMAL_ADD@",
        M::BigDecimalSubtract => "@BIGDECIMAL_SUBTRACT@", M::BigDecimalMultiply => "@BIGDECIMAL_MULTIPLY@",
        M::BigDecimalDivide => "@BIGDECIMAL_DIVIDE@", M::BigDecimalRescale => "@BIGDECIMAL_RESCALE@",
        M::BigDecimalNegate => "@BIGDECIMAL_NEGATE@", M::BigDecimalAbs => "@BIGDECIMAL_ABS@",
        M::BigDecimalToLong => "@BIGDECIMAL_TO_LONG@",
        M::RandomNew => "@RANDOM_NEW@", M::RandomFromSystem => "@RANDOM_FROM_SYSTEM@",
        M::RandomReseed => "@RANDOM_RESEED@", M::RandomNextLong => "@RANDOM_NEXT_LONG@",
        M::RandomNextInt => "@RANDOM_NEXT_INT@", M::RandomNextDouble => "@RANDOM_NEXT_DOUBLE@",
        M::CompressionDeflate => "@COMPRESSION_DEFLATE@",
        M::CompressionInflate => "@COMPRESSION_INFLATE@",
        M::CompressionGzip => "@COMPRESSION_GZIP@",
        M::CompressionGunzip => "@COMPRESSION_GUNZIP@",
        M::ChecksumsCrc32 => "@CHECKSUMS_CRC32@",
        M::ChecksumsAdler32 => "@CHECKSUMS_ADLER32@",
        M::TimeMonotonicNanos => "@TIME_MONOTONIC_NANOS@", M::TimeUnixMillis => "@TIME_UNIX_MILLIS@",
        M::TimeSleepMillis => "@TIME_SLEEP_MILLIS@", M::TimeDeadlineAfterMillis => "@TIME_DEADLINE_AFTER_MILLIS@",
        M::DateTimeFromParts => "@DATE_TIME_FROM_PARTS@",
        M::DateTimeParts => "@DATE_TIME_PARTS@",
        M::DateTimeFormat => "@DATE_TIME_FORMAT@",
        M::DateTimeParse => "@DATE_TIME_PARSE@",
        M::DateTimeIsLeapYear => "@DATE_TIME_IS_LEAP_YEAR@",
        M::DateTimeDaysInMonth => "@DATE_TIME_DAYS_IN_MONTH@",
        M::DateTimeAddCalendar => "@DATE_TIME_ADD_CALENDAR@",
        M::DateTimeDurationMillis => "@DATE_TIME_DURATION_MILLIS@",
        M::DateTimeAvailableZones => "@DATE_TIME_AVAILABLE_ZONES@",
        M::TemporalInstant => "@TEMPORAL_INSTANT@",
        M::TemporalDuration => "@TEMPORAL_DURATION@",
        M::TemporalPeriod => "@TEMPORAL_PERIOD@",
        M::TemporalLocalDate => "@TEMPORAL_LOCAL_DATE@",
        M::TemporalLocalTime => "@TEMPORAL_LOCAL_TIME@",
        M::TemporalLocalDateTime => "@TEMPORAL_LOCAL_DATE_TIME@",
        M::TemporalOffsetDateTime => "@TEMPORAL_OFFSET_DATE_TIME@",
        M::TemporalZonedDateTime => "@TEMPORAL_ZONED_DATE_TIME@",
        M::TemporalParseIso => "@TEMPORAL_PARSE_ISO@",
        M::TemporalAvailableCalendars => "@TEMPORAL_AVAILABLE_CALENDARS@",
        M::TemporalKind => "@TEMPORAL_KIND@",
        M::TemporalCalendar => "@TEMPORAL_CALENDAR@",
        M::TemporalZoneId => "@TEMPORAL_ZONE_ID@",
        M::TemporalNanosecond => "@TEMPORAL_NANOSECOND@",
        M::TemporalEpochSecond => "@TEMPORAL_EPOCH_SECOND@",
        M::TemporalOffsetSeconds => "@TEMPORAL_OFFSET_SECONDS@",
        M::TemporalFields => "@TEMPORAL_FIELDS@",
        M::TemporalFormat => "@TEMPORAL_FORMAT@",
        M::TemporalFormatIso => "@TEMPORAL_FORMAT_ISO@",
        M::TemporalCompareTo => "@TEMPORAL_COMPARE_TO@",
        M::TemporalPlusDuration => "@TEMPORAL_PLUS_DURATION@",
        M::TemporalPlusPeriod => "@TEMPORAL_PLUS_PERIOD@",
        M::TemporalAtZone => "@TEMPORAL_AT_ZONE@",
        M::TemporalWithZone => "@TEMPORAL_WITH_ZONE@",
        M::TemporalWithCalendar => "@TEMPORAL_WITH_CALENDAR@",
        M::TemporalDurationSeconds => "@TEMPORAL_DURATION_SECONDS@",
        M::TemporalPeriodYears => "@TEMPORAL_PERIOD_YEARS@",
        M::TemporalPeriodMonths => "@TEMPORAL_PERIOD_MONTHS@",
        M::TemporalPeriodDays => "@TEMPORAL_PERIOD_DAYS@",
        M::TemporalHour => "@TEMPORAL_HOUR@",
        M::TemporalMinute => "@TEMPORAL_MINUTE@",
        M::TemporalSecond => "@TEMPORAL_SECOND@",
        M::TemporalWithTime => "@TEMPORAL_WITH_TIME@",
        M::TemporalWithOffset => "@TEMPORAL_WITH_OFFSET@",
        M::SocketAddressResolve => "@SOCKET_ADDRESS_RESOLVE@",
        M::SocketAddressToText => "@SOCKET_ADDRESS_TO_TEXT@",
        M::TcpListenerBind => "@TCP_LISTENER_BIND@",
        M::TcpListenerAccept => "@TCP_LISTENER_ACCEPT@",
        M::TcpListenerLocalAddress => "@TCP_LISTENER_LOCAL_ADDRESS@",
        M::TcpListenerClose => "@TCP_LISTENER_CLOSE@",
        M::TcpStreamConnect => "@TCP_STREAM_CONNECT@",
        M::TcpStreamRead => "@TCP_STREAM_READ@",
        M::TcpStreamWrite => "@TCP_STREAM_WRITE@",
        M::TcpStreamFlush => "@TCP_STREAM_FLUSH@",
        M::TcpStreamReadSome => "@TCP_STREAM_READ_SOME@",
        M::TcpStreamWriteSome => "@TCP_STREAM_WRITE_SOME@",
        M::TcpStreamWriteAll => "@TCP_STREAM_WRITE_ALL@",
        M::TcpStreamPeerAddress => "@TCP_STREAM_PEER_ADDRESS@",
        M::TcpStreamLocalAddress => "@TCP_STREAM_LOCAL_ADDRESS@",
        M::TcpStreamClose => "@TCP_STREAM_CLOSE@",
        M::DatagramSource => "@DATAGRAM_SOURCE@",
        M::DatagramBytesRead => "@DATAGRAM_BYTES_READ@",
        M::DatagramTruncated => "@DATAGRAM_TRUNCATED@",
        M::UdpSocketBind => "@UDP_SOCKET_BIND@",
        M::UdpSocketSendTo => "@UDP_SOCKET_SEND_TO@",
        M::UdpSocketReceiveFrom => "@UDP_SOCKET_RECEIVE_FROM@",
        M::UdpSocketLocalAddress => "@UDP_SOCKET_LOCAL_ADDRESS@",
        M::UdpSocketClose => "@UDP_SOCKET_CLOSE@",
        M::HttpRequestCreate => "@HTTP_REQUEST_CREATE@",
        M::HttpRequestSetHeader => "@HTTP_REQUEST_SET_HEADER@",
        M::HttpRequestSetBody => "@HTTP_REQUEST_SET_BODY@",
        M::HttpRequestSetTimeoutMillis => "@HTTP_REQUEST_SET_TIMEOUT_MILLIS@",
        M::HttpRequestSetMaxResponseBytes => "@HTTP_REQUEST_SET_MAX_RESPONSE_BYTES@",
        M::HttpRequestSetCaFile => "@HTTP_REQUEST_SET_CA_FILE@",
        M::HttpRequestSend => "@HTTP_REQUEST_SEND@",
        M::HttpResponseStatus => "@HTTP_RESPONSE_STATUS@",
        M::HttpResponseBody => "@HTTP_RESPONSE_BODY@",
        M::HttpResponseHeader => "@HTTP_RESPONSE_HEADER@",
        M::HttpResponseFinalUrl => "@HTTP_RESPONSE_FINAL_URL@",
        M::HttpResponseBodyText => "@HTTP_RESPONSE_BODY_TEXT@",
        M::CryptoKeyGenerate => "@CRYPTO_KEY_GENERATE@",
        M::CryptoKeyParsePrivatePem => "@CRYPTO_KEY_PARSE_PRIVATE_PEM@",
        M::CryptoKeyParsePublicPem => "@CRYPTO_KEY_PARSE_PUBLIC_PEM@",
        M::CryptoKeyPublicPem => "@CRYPTO_KEY_PUBLIC_PEM@",
        M::CryptoKeyPrivatePem => "@CRYPTO_KEY_PRIVATE_PEM@",
        M::CryptoKeySign => "@CRYPTO_KEY_SIGN@",
        M::CryptoKeyVerify => "@CRYPTO_KEY_VERIFY@",
        M::CryptoKeyAgree => "@CRYPTO_KEY_AGREE@",
        M::CryptoDigestCreate => "@CRYPTO_DIGEST_CREATE@",
        M::CryptoDigestUpdate => "@CRYPTO_DIGEST_UPDATE@",
        M::CryptoDigestFinish => "@CRYPTO_DIGEST_FINISH@",
        M::CryptoMacCreate => "@CRYPTO_MAC_CREATE@",
        M::CryptoMacUpdate => "@CRYPTO_MAC_UPDATE@",
        M::CryptoMacFinish => "@CRYPTO_MAC_FINISH@",
        M::CertificateParsePem => "@CERTIFICATE_PARSE_PEM@",
        M::CertificateParsePemChain => "@CERTIFICATE_PARSE_PEM_CHAIN@",
        M::CertificateToPem => "@CERTIFICATE_TO_PEM@",
        M::CertificateToPemChain => "@CERTIFICATE_TO_PEM_CHAIN@",
        M::CertificateSubject => "@CERTIFICATE_SUBJECT@",
        M::CertificateIssuer => "@CERTIFICATE_ISSUER@",
        M::CertificateSerialText => "@CERTIFICATE_SERIAL_TEXT@",
        M::CertificateNotBeforeUnixSeconds => "@CERTIFICATE_NOT_BEFORE_UNIX_SECONDS@",
        M::CertificateNotAfterUnixSeconds => "@CERTIFICATE_NOT_AFTER_UNIX_SECONDS@",
        M::CertificateMatchesHost => "@CERTIFICATE_MATCHES_HOST@",
        M::CertificateVerify => "@CERTIFICATE_VERIFY@",
        M::UriParse => "@URI_PARSE@", M::UriFromIri => "@URI_FROM_IRI@",
        M::UriClone => "@URI_CLONE@", M::UriToText => "@URI_TO_TEXT@",
        M::UriScheme => "@URI_SCHEME@", M::UriUserInfo => "@URI_USER_INFO@",
        M::UriHost => "@URI_HOST@", M::UriPath => "@URI_PATH@",
        M::UriQuery => "@URI_QUERY@", M::UriFragment => "@URI_FRAGMENT@",
        M::UriPort => "@URI_PORT@", M::UriIsAbsolute => "@URI_IS_ABSOLUTE@",
        M::UriNormalize => "@URI_NORMALIZE@", M::UriResolve => "@URI_RESOLVE@",
        M::UriRelativize => "@URI_RELATIVIZE@", M::UriEquals => "@URI_EQUALS@",
        M::UriEncodeComponent => "@URI_ENCODE_COMPONENT@",
        M::UriDecodeComponent => "@URI_DECODE_COMPONENT@",
        M::UriPercentDecode => "@URI_PERCENT_DECODE@",
        M::DnsIdna => "@DNS_IDNA@", M::DnsResolve => "@DNS_RESOLVE@",
        M::DnsReverse => "@DNS_REVERSE@", M::DnsLocalHostName => "@DNS_LOCAL_HOST_NAME@",
        M::ZipReaderOpen => "@ZIP_READER_OPEN@",
        M::ZipReaderFromBytes => "@ZIP_READER_FROM_BYTES@",
        M::ZipReaderNext => "@ZIP_READER_NEXT@",
        M::ZipReaderReadEntry => "@ZIP_READER_READ_ENTRY@",
        M::ZipReaderSkipEntry => "@ZIP_READER_SKIP_ENTRY@",
        M::ZipReaderClose => "@ZIP_READER_CLOSE@",
        M::ZipEntryName => "@ZIP_ENTRY_NAME@",
        M::ZipEntrySize => "@ZIP_ENTRY_SIZE@",
        M::ZipEntryIsDirectory => "@ZIP_ENTRY_IS_DIRECTORY@",
        M::ZipWriterCreate => "@ZIP_WRITER_CREATE@",
        M::ZipWriterAdd => "@ZIP_WRITER_ADD@",
        M::ZipWriterAddDirectory => "@ZIP_WRITER_ADD_DIRECTORY@",
        M::ZipWriterClose => "@ZIP_WRITER_CLOSE@",
        M::ThreadSpawn => "@THREAD_SPAWN@", M::ThreadJoin => "@THREAD_JOIN@",
        M::MutexCreate => "@MUTEX_CREATE@", M::MutexCloneHandle => "@MUTEX_CLONE_HANDLE@",
        M::MutexLock => "@MUTEX_LOCK@", M::MutexTryLock => "@MUTEX_TRY_LOCK@",
        M::GuardGetRef => "@GUARD_GET_REF@", M::GuardGetMut => "@GUARD_GET_MUT@",
        M::GuardRead => "@GUARD_READ@", M::GuardWrite => "@GUARD_WRITE@",
        M::ChannelBounded => "@CHANNEL_BOUNDED@", M::ChannelTakeSender => "@CHANNEL_TAKE_SENDER@",
        M::ChannelTakeReceiver => "@CHANNEL_TAKE_RECEIVER@",
        M::SenderSend => "@SENDER_SEND@", M::SenderCloneSender => "@SENDER_CLONE_SENDER@",
        M::SenderClose => "@SENDER_CLOSE@", M::ReceiverReceive => "@RECEIVER_RECEIVE@",
        M::ReceiverClose => "@RECEIVER_CLOSE@",
        M::AtomicCreate => "@ATOMIC_CREATE@", M::AtomicCloneHandle => "@ATOMIC_CLONE_HANDLE@",
        M::AtomicLoad => "@ATOMIC_LOAD@", M::AtomicStore => "@ATOMIC_STORE@",
        M::AtomicExchange => "@ATOMIC_EXCHANGE@", M::AtomicCompareExchange => "@ATOMIC_COMPARE_EXCHANGE@",
        M::AtomicFetchAdd => "@ATOMIC_FETCH_ADD@", M::AtomicFetchSubtract => "@ATOMIC_FETCH_SUBTRACT@",
        M::SemaphoreCreate => "@SEMAPHORE_CREATE@", M::SemaphoreCloneHandle => "@SEMAPHORE_CLONE_HANDLE@",
        M::SemaphoreAcquire => "@SEMAPHORE_ACQUIRE@", M::SemaphoreRelease => "@SEMAPHORE_RELEASE@",
        M::LatchCreate => "@LATCH_CREATE@", M::LatchCloneHandle => "@LATCH_CLONE_HANDLE@",
        M::LatchCountDown => "@LATCH_COUNT_DOWN@", M::LatchWait => "@LATCH_WAIT@", M::LatchCount => "@LATCH_COUNT@",
        M::BarrierCreate => "@BARRIER_CREATE@", M::BarrierCloneHandle => "@BARRIER_CLONE_HANDLE@",
        M::BarrierWait => "@BARRIER_WAIT@", M::BarrierReset => "@BARRIER_RESET@",
        M::ConditionCreate => "@CONDITION_CREATE@", M::ConditionCloneHandle => "@CONDITION_CLONE_HANDLE@",
        M::ConditionWait => "@CONDITION_WAIT@", M::ConditionSignal => "@CONDITION_SIGNAL@",
        M::ConditionBroadcast => "@CONDITION_BROADCAST@",
        M::CancellationTokenCreate => "@CANCELLATION_TOKEN_CREATE@",
        M::CancellationTokenCloneHandle => "@CANCELLATION_TOKEN_CLONE_HANDLE@",
        M::CancellationTokenRequest => "@CANCELLATION_TOKEN_REQUEST@",
        M::CancellationTokenIsCancelled => "@CANCELLATION_TOKEN_IS_CANCELLED@",
        M::FutureSpawn => "@FUTURE_SPAWN@", M::FutureAwait => "@FUTURE_AWAIT@",
        M::FutureCancel => "@FUTURE_CANCEL@", M::FutureIsDone => "@FUTURE_IS_DONE@",
        M::ThreadPoolCreate => "@THREAD_POOL_CREATE@", M::ThreadPoolSubmit => "@THREAD_POOL_SUBMIT@",
        M::ThreadPoolSchedule => "@THREAD_POOL_SCHEDULE@",
        M::ThreadPoolScheduleAtFixedRate => "@THREAD_POOL_SCHEDULE_AT_FIXED_RATE@",
        M::ThreadPoolScheduleWithFixedDelay => "@THREAD_POOL_SCHEDULE_WITH_FIXED_DELAY@",
        M::ThreadPoolShutdown => "@THREAD_POOL_SHUTDOWN@",
        M::EnvGet => "@ENV_GET@", M::EnvSet => "@ENV_SET@", M::EnvUnset => "@ENV_UNSET@",
        M::CommandCreate => "@COMMAND_CREATE@", M::CommandAddArg => "@COMMAND_ADD_ARG@",
        M::CommandSetEnv => "@COMMAND_SET_ENV@", M::CommandRedirectStdin => "@COMMAND_REDIRECT_STDIN@",
        M::CommandRedirectStdout => "@COMMAND_REDIRECT_STDOUT@",
        M::CommandRedirectStderr => "@COMMAND_REDIRECT_STDERR@",
        M::CommandClearEnv => "@COMMAND_CLEAR_ENV@", M::CommandSpawn => "@COMMAND_SPAWN@",
        M::CommandOutput => "@COMMAND_OUTPUT@",
        M::ProcessWait => "@PROCESS_WAIT@", M::ProcessWaitFor => "@PROCESS_WAIT_FOR@",
        M::ProcessTerminate => "@PROCESS_TERMINATE@", M::ProcessKill => "@PROCESS_KILL@",
        M::ProcessOutputStdout => "@PROCESS_OUTPUT_STDOUT@",
        M::ProcessOutputStderr => "@PROCESS_OUTPUT_STDERR@",
        M::ProcessOutputStatus => "@PROCESS_OUTPUT_STATUS@",
        M::ProcessOutputStdoutTruncated => "@PROCESS_OUTPUT_STDOUT_TRUNCATED@",
        M::ProcessOutputStderrTruncated => "@PROCESS_OUTPUT_STDERR_TRUNCATED@",
        M::XmlDocumentParse => "@XML_DOCUMENT_PARSE@",
        M::XmlDocumentSerialize => "@XML_DOCUMENT_SERIALIZE@",
        M::XmlDocumentRootName => "@XML_DOCUMENT_ROOT_NAME@",
        M::XmlDocumentQueryText => "@XML_DOCUMENT_QUERY_TEXT@",
        M::XmlDocumentQueryXml => "@XML_DOCUMENT_QUERY_XML@",
        M::XmlDocumentSetText => "@XML_DOCUMENT_SET_TEXT@",
        M::XmlDocumentSetAttribute => "@XML_DOCUMENT_SET_ATTRIBUTE@",
        M::XmlDocumentRemove => "@XML_DOCUMENT_REMOVE@",
        M::XmlDocumentAppendXml => "@XML_DOCUMENT_APPEND_XML@",
        M::XmlDocumentValidate => "@XML_DOCUMENT_VALIDATE@",
        M::XmlDocumentRegisterNamespace => "@XML_DOCUMENT_REGISTER_NAMESPACE@",
        M::XmlDocumentEscape => "@XML_DOCUMENT_ESCAPE@",
        M::ProcessOutputTimedOut => "@PROCESS_OUTPUT_TIMED_OUT@",
        M::TlsConfigClient => "@TLS_CONFIG_CLIENT@",
        M::TlsConfigClientWithCa => "@TLS_CONFIG_CLIENT_WITH_CA@",
        M::TlsConfigClientWithIdentity => "@TLS_CONFIG_CLIENT_WITH_IDENTITY@",
        M::TlsConfigServer => "@TLS_CONFIG_SERVER@",
        M::TlsConfigServerWithClientAuth => "@TLS_CONFIG_SERVER_WITH_CLIENT_AUTH@",
        M::TlsStreamConnect => "@TLS_STREAM_CONNECT@",
        M::TlsStreamAccept => "@TLS_STREAM_ACCEPT@",
        M::TlsStreamReadSome => "@TLS_STREAM_READ_SOME@",
        M::TlsStreamWriteSome => "@TLS_STREAM_WRITE_SOME@",
        M::TlsStreamWriteAll => "@TLS_STREAM_WRITE_ALL@",
        M::TlsStreamCipher => "@TLS_STREAM_CIPHER@",
        M::TlsStreamProtocol => "@TLS_STREAM_PROTOCOL@",
        M::TlsStreamPeerCertificate => "@TLS_STREAM_PEER_CERTIFICATE@",
        M::TlsStreamClose => "@TLS_STREAM_CLOSE@",
        M::TlsStreamRead => "@TLS_STREAM_READ@",
        M::TlsStreamWrite => "@TLS_STREAM_WRITE@",
        M::TlsStreamFlush => "@TLS_STREAM_FLUSH@",
        M::WebSocketConnect => "@WEBSOCKET_CONNECT@",
        M::WebSocketConnectTls => "@WEBSOCKET_CONNECT_TLS@",
        M::WebSocketSendText => "@WEBSOCKET_SEND_TEXT@",
        M::WebSocketSendBinary => "@WEBSOCKET_SEND_BINARY@",
        M::WebSocketPing => "@WEBSOCKET_PING@",
        M::WebSocketReceive => "@WEBSOCKET_RECEIVE@",
        M::WebSocketClose => "@WEBSOCKET_CLOSE@",
        M::WebSocketMessageIsText => "@WEBSOCKET_MESSAGE_IS_TEXT@",
        M::WebSocketMessageBody => "@WEBSOCKET_MESSAGE_BODY@",
        M::WebSocketMessageAsText => "@WEBSOCKET_MESSAGE_AS_TEXT@",
        M::XmlReaderFromText => "@XML_READER_FROM_TEXT@",
        M::XmlReaderFromBuffer => "@XML_READER_FROM_BUFFER@",
        M::XmlReaderRead => "@XML_READER_READ@",
        M::XmlReaderClose => "@XML_READER_CLOSE@",
        M::XmlEventNodeType => "@XML_EVENT_NODE_TYPE@",
        M::XmlEventDepth => "@XML_EVENT_DEPTH@",
        M::XmlEventIsEmptyElement => "@XML_EVENT_IS_EMPTY_ELEMENT@",
        M::XmlEventName => "@XML_EVENT_NAME@",
        M::XmlEventLocalName => "@XML_EVENT_LOCAL_NAME@",
        M::XmlEventNamespaceUri => "@XML_EVENT_NAMESPACE_URI@",
        M::XmlEventValue => "@XML_EVENT_VALUE@",
        M::XmlEventAttributeCount => "@XML_EVENT_ATTRIBUTE_COUNT@",
        M::XmlEventAttributeName => "@XML_EVENT_ATTRIBUTE_NAME@",
        M::XmlEventAttributeNamespaceUri => "@XML_EVENT_ATTRIBUTE_NAMESPACE_URI@",
        M::XmlEventAttributeValue => "@XML_EVENT_ATTRIBUTE_VALUE@",
        M::XmlWriterCreate => "@XML_WRITER_CREATE@",
        M::XmlWriterStartDocument => "@XML_WRITER_START_DOCUMENT@",
        M::XmlWriterStartElement => "@XML_WRITER_START_ELEMENT@",
        M::XmlWriterStartElementNs => "@XML_WRITER_START_ELEMENT_NS@",
        M::XmlWriterWriteAttribute => "@XML_WRITER_WRITE_ATTRIBUTE@",
        M::XmlWriterWriteAttributeNs => "@XML_WRITER_WRITE_ATTRIBUTE_NS@",
        M::XmlWriterWriteText => "@XML_WRITER_WRITE_TEXT@",
        M::XmlWriterWriteCData => "@XML_WRITER_WRITE_CDATA@",
        M::XmlWriterWriteComment => "@XML_WRITER_WRITE_COMMENT@",
        M::XmlWriterWriteProcessingInstruction => "@XML_WRITER_WRITE_PI@",
        M::XmlWriterEndElement => "@XML_WRITER_END_ELEMENT@",
        M::XmlWriterFinish => "@XML_WRITER_FINISH@",
        M::XsltStylesheetCompile => "@XSLT_STYLESHEET_COMPILE@",
        M::XsltStylesheetTransform => "@XSLT_STYLESHEET_TRANSFORM@",
        M::XsltStylesheetTransformBytes => "@XSLT_STYLESHEET_TRANSFORM_BYTES@",
    }
}
pub(super) fn template_section<'a>(source: &'a str, begin: &str, end: &str) -> &'a str {
    source.split_once(begin).expect("native template section opener").1
        .split_once(end).expect("native template section closer").0
}
pub(super) fn required_indexed_native_method<'a>(
    program: &'a t::Program,
    method_ids: &t::NativeMethodIndex,
    receiver: Option<usize>,
    method: M,
) -> &'a t::Function {
    let id = match receiver {
        Some(receiver) => method_ids.get(receiver, method),
        None => method_ids.get_static(method),
    }.unwrap_or_else(|| panic!("compiler-owned native method {method:?} is present"));
    program.functions.get(id).expect("native method index contains a valid function ID")
}

pub(super) fn native_function_if(program: &t::Program, is: impl Fn(M) -> bool) -> Option<&t::Function> {
    program.functions.iter().find(|function| {
        function.native.is_some_and(&is)
    })
}
pub(super) fn native_function(program: &t::Program, is: impl Fn(M) -> bool) -> &t::Function {
    native_function_if(program, is).expect("compiler-owned native function is present")
}
/// Receiver-scoped lookup; static factories use their own qualification rules.
pub(super) fn native_method_if(
    program: &t::Program, receiver: usize, is: impl Fn(M) -> bool,
) -> Option<&t::Function> {
    program.functions.iter().find(|function| {
        function.receiver == Some(receiver) && function.native.is_some_and(&is)
    })
}
pub(super) fn native_method(
    program: &t::Program, receiver: usize, is: impl Fn(M) -> bool,
) -> &t::Function {
    native_method_if(program, receiver, is).expect("compiler-owned native method is present")
}
pub(super) fn native_class_id_if(program: &t::Program, is: impl Fn(C) -> bool) -> Option<usize> {
    program.classes.iter().find(|class| class.native.is_some_and(&is)).map(|class| class.id)
}
pub(super) fn native_class_id(program: &t::Program, is: impl Fn(C) -> bool) -> usize {
    native_class_id_if(program, is).expect("compiler-owned native class is present")
}

pub(super) fn result_id(function: &t::Function) -> usize {
    match function.return_type {
        Type::Object(id) => id,
        _ => unreachable!("native result is a concrete sum object"),
    }
}
pub(super) fn variant<'a>(program: &'a t::Program, class: usize, name: &str) -> (usize, &'a t::Variant) {
    program.classes[class].variants.as_ref().expect("sum class variants")
        .iter().enumerate().find(|(_, variant)| variant.name == name)
        .expect("compiler-owned native enum variant")
}
pub(super) fn variant_id(program: &t::Program, class: usize, name: &str) -> usize {
    variant(program, class, name).0
}

pub(super) fn drop_element(ty: Type, expression: &str) -> String {
    match ty {
        Type::Object(id) => format!("ncr_drop_{id}({expression});"),
        _ => "/* Copy value: no destructor. */".to_owned(),
    }
}

pub(super) fn zero(program: &t::Program, ty: Type) -> String {
    match ty {
        Type::Byte | Type::Unit => "UINT8_C(0)".to_owned(),
        Type::Char => "UINT32_C(0)".to_owned(),
        Type::Long => "INT64_C(0)".to_owned(),
        Type::Double => "0.0".to_owned(),
        Type::Int => "INT32_C(0)".to_owned(),
        Type::Boolean => "false".to_owned(),
        Type::String => "(ncr_string){NULL, 0}".to_owned(),
        Type::Object(id) => crate::c_repr::object_zero(program, id),
        _ => unreachable!("checked native collection value"),
    }
}

pub(super) fn option_tags(
    program: &t::Program,
    function: &t::Function,
    element: Type,
) -> (usize, usize) {
    let option = result_id(function);
    let (none_tag, none) = variant(program, option, "None");
    let (some_tag, some) = variant(program, option, "Some");
    assert!(none.fields.is_empty(), "Option None must have no payload");
    assert!(
        matches!(some.fields.as_slice(), [field] if *field == element),
        "Option Some payload must be exactly its element type",
    );
    (none_tag, some_tag)
}

pub(super) fn text_class(program: &t::Program) -> Option<usize> {
    program.classes.iter().find(|class| matches!(class.native, Some(C::Text))).map(|class| class.id)
}
pub(super) fn text_helpers(program: &t::Program, text: usize) -> String {
    let text_new = native_function(program, |method| matches!(method, M::TextNew)).id;
    let mut out = format!(
        "static void ncr_text_init_{text}(ncr_obj_{text} *p) {{\n    p->bytes = p->inline_storage;\n    p->length = 0;\n    p->capacity = {TEXT_INLINE_CAPACITY};\n}}\n"
    );
    out.push_str(&format!(
        "static ncr_obj_{text} *ncr_text_from_bytes_{text}(const unsigned char *bytes, size_t length, size_t line, size_t column) {{\n    ncr_obj_{text} *text = ncr_allocate(sizeof(ncr_obj_{text}), line, column);\n    ncr_fn_{text_new}(text, (ncr_string){{bytes, length}}, line, column);\n    return text;\n}}\n"
    ));
    out.push_str(&format!(
        "static ncr_obj_{text} *ncr_text_take_bytes_{text}(unsigned char *bytes, size_t length, size_t capacity, size_t line, size_t column) {{\n    if (length > (size_t)INT32_MAX || capacity > (size_t)INT32_MAX) {{ ncr_fail(\"Text is too large\", line, column); }}\n    if (length > capacity || (length > 0 && bytes == NULL)) {{ ncr_fail(\"invalid owned Text storage\", line, column); }}\n    ncr_obj_{text} *text = ncr_allocate(sizeof(ncr_obj_{text}), line, column);\n    ncr_text_init_{text}(text);\n    if (length <= {TEXT_INLINE_CAPACITY}) {{\n        if (length > 0) {{ memcpy(text->inline_storage, bytes, length); }}\n        text->length = (int32_t)length;\n        if (bytes != NULL) {{ ncr_deallocate(bytes); }}\n    }} else {{\n        text->bytes = bytes;\n        text->length = (int32_t)length;\n        text->capacity = (int32_t)capacity;\n    }}\n    return text;\n}}\n"
    ));
    let mut result_ids = Vec::new();
    for function in &program.functions {
        if function.native.is_none() { continue; }
        let Type::Object(class) = function.return_type else { continue; };
        let Some(layout) = program.classes[class].result else { continue; };
        if layout.err_type != Type::Object(text) { continue; }
        if !result_ids.contains(&class) { result_ids.push(class); }
    }
    for class in result_ids {
        let err = program.classes[class].result.expect("trusted Result layout").err_variant;
        out.push_str(&format!(
            "static ncr_obj_{class} ncr_result_error_{class}(const char *message, size_t length, size_t line, size_t column) {{\n    ncr_obj_{text} *error = ncr_text_from_bytes_{text}((const unsigned char *)message, length, line, column);\n    return (ncr_obj_{class}){{ .tag = {err}, .data.v{err}.f0 = error }};\n}}\n"
        ));
    }
    out
}
fn next_placeholder(source: &str, search_from: usize) -> Option<(usize, usize)> {
    let start = search_from + source[search_from..].find('@')?;
    let relative_end = source[start + 1..].find('@')?;
    Some((start, start + relative_end + 2))
}

pub(super) fn replace_method_ids(
    body: String,
    program: &t::Program,
    receiver: Option<usize>,
) -> String {
    let mut placeholders = HashSet::new();
    let mut previous_at = None;
    for (index, byte) in body.bytes().enumerate() {
        if byte == b'@' {
            if let Some(start) = previous_at {
                // Keep overlapping pairs so an unmatched outer placeholder cannot hide an inner one.
                placeholders.insert(&body[start..=index]);
            }
            previous_at = Some(index);
        }
    }
    if placeholders.is_empty() { return body; }

    let mut substitutions = Vec::new();
    for function in &program.functions {
        let Some(method) = function.native else { continue; };
        let placeholder = marker(method);
        if !placeholders.contains(placeholder) { continue; }
        let receiver_matches = if let Some(class) = receiver {
            let is_static_factory = function.receiver.is_none()
                && matches!(method, M::MapWithHasher | M::SetWithHasher)
                && function.name.strip_prefix(program.classes[class].name.as_str())
                    == Some(".withHasher");
            function.receiver == Some(class) || is_static_factory
        } else {
            true
        };
        if receiver_matches && placeholders.remove(placeholder) {
            substitutions.push((placeholder, format!("ncr_fn_{}", function.id)));
        }
    }
    drop(placeholders);
    replace_placeholders(body, &substitutions)
}

pub(super) fn replace_placeholders(
    body: String,
    substitutions: &[(impl AsRef<str>, impl AsRef<str>)],
) -> String {
    let has_nested_values = substitutions.iter().enumerate().any(|(index, (_, value))| {
        substitutions[index + 1..].iter()
            .any(|(placeholder, _)| value.as_ref().contains(placeholder.as_ref()))
    });
    if has_nested_values {
        let mut body = body;
        for (placeholder, replacement) in substitutions {
            let placeholder = placeholder.as_ref();
            let replacement = replacement.as_ref();
            if body.contains(placeholder) {
                body = body.replace(placeholder, replacement);
            }
        }
        return body;
    }
    let mut output = None;
    let mut copied_until = 0;
    let mut search_from = 0;
    while let Some((start, end)) = next_placeholder(&body, search_from) {
        let marker = &body[start..end];
        let replacement = substitutions.iter()
            .find(|(placeholder, _)| placeholder.as_ref() == marker)
            .map(|(_, replacement)| replacement.as_ref());
        let Some(replacement) = replacement else {
            search_from = start + 1;
            continue;
        };
        let output = output.get_or_insert_with(|| String::with_capacity(body.len()));
        output.push_str(&body[copied_until..start]);
        output.push_str(replacement);
        copied_until = end;
        search_from = end;
    }
    match output {
        Some(mut output) => {
            output.push_str(&body[copied_until..]);
            output
        }
        None => body,
    }
}

#[cfg(test)]
mod tests {
    use super::{replace_method_ids, replace_placeholders};

    fn function(
        id: usize,
        name: &str,
        receiver: Option<usize>,
        native: super::M,
    ) -> super::t::Function {
        super::t::Function {
            native: Some(native),
            virtual_slot: None,
            id,
            name: name.to_owned(),
            receiver,
            readonly: false,
            return_type: super::Type::Unit,
            locals: Vec::new(),
            params: Vec::new(),
            body: Vec::new(),
        }
    }

    fn class(id: usize, name: &str) -> super::t::Class {
        super::t::Class {
            native: None,
            id,
            name: name.to_owned(),
            fields: Vec::new(),
            variants: None,
            result: None,
            has_borrow: false,
            is_interface: false,
            interface_methods: Vec::new(),
            implementations: Vec::new(),
            borrowed_from: Vec::new(),
        }
    }

    #[test]
    fn replacement_values_expand_in_substitution_order() {
        let result = replace_placeholders(
            "π=@FIRST@ | @MISSING@ | @FIRST@".to_owned(),
            &[
                ("@FIRST@", "value @SECOND@".to_owned()),
                ("@SECOND@", "second".to_owned()),
            ],
        );
        assert_eq!(result, "π=value second | @MISSING@ | value second");
    }

    #[test]
    fn earlier_placeholders_do_not_reprocess_later_values() {
        let result = replace_placeholders(
            "@FIRST@".to_owned(),
            &[
                ("@SECOND@", "second".to_owned()),
                ("@FIRST@", "@SECOND@".to_owned()),
            ],
        );
        assert_eq!(result, "@SECOND@");
    }

    #[test]
    fn direct_replacements_expand_all_matches_once() {
        let result = replace_placeholders(
            "π=@ID@ / @ID@ / @UNKNOWN@".to_owned(),
            &[("@ID@", "7".to_owned())],
        );
        assert_eq!(result, "π=7 / 7 / @UNKNOWN@");
    }

    #[test]
    fn adjacent_placeholders_are_independent() {
        let result = replace_placeholders(
            "@LEFT@@RIGHT@".to_owned(),
            &[("@LEFT@", "L".to_owned()), ("@RIGHT@", "R".to_owned())],
        );
        assert_eq!(result, "LR");
    }

    #[test]
    fn method_ids_keep_first_match_and_respect_static_factory_receiver() {
        let program = super::t::Program {
            classes: vec![class(0, "Map"), class(1, "Other")],
            functions: vec![
                function(10, "Map.withHasher", Some(1), super::M::MapWithHasher),
                function(20, "Map.withHasher", None, super::M::MapWithHasher),
                function(30, "Map.withHasher", None, super::M::MapWithHasher),
                function(40, "Other.withHasher", None, super::M::MapWithHasher),
                function(50, "Map.size", Some(0), super::M::MapSize),
                function(60, "Map.size", Some(0), super::M::MapSize),
            ],
            main: 0,
        };
        let with_hasher = super::marker(super::M::MapWithHasher);
        let map_size = super::marker(super::M::MapSize);
        assert_eq!(
            replace_method_ids(with_hasher.to_owned(), &program, None),
            "ncr_fn_10",
        );
        assert_eq!(
            replace_method_ids(with_hasher.to_owned(), &program, Some(0)),
            "ncr_fn_20",
        );
        assert_eq!(
            replace_method_ids(
                format!("{with_hasher}{map_size}{with_hasher}{map_size}"),
                &program,
                Some(0),
            ),
            "ncr_fn_20ncr_fn_50ncr_fn_20ncr_fn_50",
        );
        assert_eq!(
            replace_method_ids(format!("@UNKNOWN{map_size}"), &program, Some(0)),
            "@UNKNOWNncr_fn_50",
        );
    }

    #[test]
    fn unmatched_placeholders_are_preserved() {
        let input = "before @UNMATCHED@ after";
        assert_eq!(
            replace_placeholders(input.to_owned(), &[("@KNOWN@", "value".to_owned())]),
            input,
        );
    }
}


pub(super) fn has_placeholder(source: &str) -> bool {
    let bytes = source.as_bytes();
    for (start, byte) in bytes.iter().enumerate() {
        if *byte != b'@' { continue; }
        let name = &bytes[start + 1..];
        let length = name.iter().take_while(|byte|
            byte.is_ascii_uppercase() || byte.is_ascii_digit() || **byte == b'_').count();
        if length > 0 && name.get(length) == Some(&b'@') { return true; }
    }
    false
}
