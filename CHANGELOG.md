# Changelog

All notable changes to Orchid are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/) once tagged
releases begin. Until then, entries under **Unreleased** describe the pre-alpha
tree on [`main`](https://github.com/ionpmp/Orchid).

## [Unreleased]

Pre-alpha snapshot as of **2026-10-08** (`0.1.0` workspace version). No tagged
release yet.

### Added

#### Workspace & shell
- **Protection widget:** traces, histories, free-space overwrite, and
  per-program outbound firewall blocks. Scan before delete. Cookies, Windows
  Temp, and the advertising id stay off until checked. Browser history keeps
  bookmarks. Free space is overwritten with zeros, leaving 64 MB, and can be
  cancelled. Network blocks are outbound Windows Firewall rules named
  `Orchid Protect` and skip system processes. History, cache, and cookies
  stay while that browser is open. Clean shows the size of the checked rows
  after a scan.
- **Protection network:** an executable that is not running can be blocked
  from the file dialog. System directories, security processes, and Orchid
  are still refused.
- **Optimize widget:** search, a Changed mark, three presets (As Windows,
  Don't reboot itself, Quiet desktop), and a Startup tab that disables Run
  entries and Startup-folder shortcuts without deleting them. New switches
  pin the current Windows version, stop Store auto-updates, turn off fast
  startup, skip accessibility prompts, turn off mouse acceleration, open
  Explorer folders in separate processes, show clock seconds, and add End
  task to the taskbar.
- **Optimize widget:** Windows update, privacy, Explorer, taskbar, suggestion,
  and performance switches in one place. The set is the reversible overlap of
  Winaero Tweaker, the recommended O&O ShutUp10 switches, and Microsoft Update
  policy, including no restart while you are signed in. Administrator changes
  ask Windows to confirm. Defender, the firewall, and the Windows Update
  service stay as they are.
- **Calendar sync:** the calendar can save CalDAV collection URLs,
  a user, and a password. Extra URLs in the same field, separated by a
  space or a new line, use that account. Sync covers 90 days ago through
  the next year. Daily, weekly, monthly, and yearly repeats, with interval,
  count, until, and weekly weekdays, are shown on each day. A date listed
  in EXDATE is left out. Other rule parts and exception forms are ignored. A multi-day event is shown on each day, up to 14 days
  and 400 occurrences. Editing or deleting one of those days stays on this
  computer and does not change the series; the next sync restores it.
  A one-time event still writes edits and deletes back. A `Z` time is
  shown in the local offset; a named timezone is stored as written. The
  password is kept with the widget config (DPAPI on Windows) and is not
  shown again. There is no server discovery and no OAuth.
- **Contacts:** local cards plus one CardDAV collection. A card stores a
  name, two emails, two phone numbers, and a note. A third address is
  dropped. Cards saved earlier still open, with the new fields empty.
  Sync uses basic authentication and replaces linked cards with the server copy. A card
  that has not been uploaded stays on this computer. Photos, groups,
  address-book discovery, and OAuth are not included. At most 500 cards.
- **Mail widget:** IMAP/SMTP client with an account wizard (built-in
  profiles, Mozilla ISPDB, DNS SRV), DPAPI-stored secrets, SQLite header
  cache, three-pane reading, HTML via WebView2, compose/reply/forward,
  and OAuth hooks for Gmail / Microsoft 365.
- **Mail client:** IMAP can use implicit TLS or STARTTLS. Cleartext login
  is refused. The search box filters the open folder and runs an IMAP
  `TEXT` search. Messages group by subject after reply prefixes are
  removed. The background refresh uses IMAP IDLE for about 90 seconds and
  syncs again when new mail arrives. Toolbar Refresh does not wait on IDLE.
  Opening a message keeps attachment names and bytes in the local cache.
  The reading pane lists those names, and Save writes the cached bytes.
  There is no preview. Compose can attach up to 10 files and 25 MB from
  this computer. Send and Save draft read the files then. A few extensions
  get a matching type; other files are `application/octet-stream`. There
  is no preview and no drag-and-drop. Forward does not copy the original
  files. Compose has a Bcc line. The open message can move to another
  folder the account already lists, one message at a time. Mark read sets
  the seen flag on unread messages in the open folder's current list, the
  latest 100 headers. Messages outside that list stay unread. Mark unread
  clears that flag on messages in the same list that are already read.
  Messages outside that list keep their flags.
  Rules and Microsoft Graph are not included.
- **Agent conversation:** Universal Search `?` and the Agent widget share
  `data/agent-chat.json`. The model can read a local text file, list one
  folder, and search the open index. A proposed file write is stored until
  you confirm it. There is no shell, and the transcript is not uploaded.
- **Process graphs:** the Processes tab draws the last 60 CPU and memory
  samples for the selected process. CPU is that process's percent. Memory
  bars are scaled to the peak working set in the window. The samples stay
  in memory and are not written to disk.
- **Spreadsheets:** `.xlsx` and `.xlsm` open as a sheet and cell table.
  The preview highlights at most 8 conditional formatting rules. Each range
  covers at most 32 cells. A rule can be greater than a plain number, equal
  to a plain number or a quoted string, or a containsText pattern using
  `*`, `?`, and `~`. A pattern without those characters is an exact ASCII
  case-insensitive match, not a substring search. A pattern longer than 64
  characters, or cell text longer than 256 characters, is not a match. The
  first matching rule wins. A formula rule is one comparison of a cell
  address to a plain number, using `>`, `<`, `=`, `>=`, `<=`, or `<>`.
  The address shifts from the first cell of the range, and `$` locks that
  side. The compared value is that cell's preview text, read as a plain
  number. A function, text, another sheet, or any other formula is skipped
  and does not count toward the 8. A color scale is separate from those 8
  rules. At most 4 scales are drawn. Each range is at most 32 cells. A scale
  has two or three stops: min, max, a number, or a percentile from 0 through
  100. The color is an `rgb` hex value, `RRGGBB` or `AARRGGBB`, blended in
  RGB from the cell's preview text read as a plain number. A formula stop, a
  theme color, more than three stops, a stop that falls, or a range larger
  than 32 cells leaves that scale undrawn and does not count toward the 4.
  The first scale that covers a cell wins. When every stop is the same
  number, the first color is used. Hidden rows are already omitted, so they
  are not part of min, max, or a percentile. A data bar is also separate
  from those 8 rules and from the 4 scales. At most 4 bars are drawn. Each
  range is at most 32 cells. A bar has two stops: min, max, a number, or a
  percentile from 0 through 100. The length is that cell's preview number
  as a percent of the span from the low stop to the high stop, from 0
  through 100. A value below the low stop draws nothing and a value above
  the high stop fills the cell. When both stops are the same number, the
  bar is full. The color is one solid `rgb` hex value. A bar with no color
  uses 638EC6. There is no gradient, border, axis, or separate negative
  color. `minLength` and `maxLength` are not applied. A formula stop, a
  theme color, more or fewer than two stops, a stop that falls, or a range
  larger than 32 cells leaves that bar undrawn and does not count toward
  the 4. The first bar that covers a cell wins. Hidden rows are already
  omitted. Icon sets are still skipped. The bar is not written back. The
  fill is not written back. A range larger than 32
  cells is skipped. The highlight is not written back and formulas do not
  use it. A selected cell can show a legacy comment from the worksheet
  comments part. At most 32 comments are read, each note is at most 256
  characters, and the author is ignored. Threaded comments are read from
  the threaded-comment part. At most 32 are read. Each note is at most 256
  characters. Replies on the same cell are joined with a newline, then cut
  to 256 characters. The person id is ignored. A legacy comment on the same
  cell is the one shown. The note is not editable and is not written back. The preview reads at
  most 16 merged ranges. Each range covers at most 8 columns and 8 rows.
  The origin cell widens across those columns, and the other cells in the
  range are hidden. The row does not grow taller. Column widths come from
  the worksheet `cols` list, clamped from 1 to 40 character units and drawn
  at 8 pixels per unit. A missing width stays 72 pixels. A merge or a width
  is not written back. A frozen pane stays on screen while the rest of the
  preview scrolls. The first `pane` in the first `sheetView` is used. Its
  `state` must be `frozen` or `frozenSplit`. `ySplit` pins that many preview
  rows, clamped to 8, and `xSplit` pins that many preview columns, clamped
  to 4. A split that is not frozen pins nothing. Hidden rows are already
  omitted, so the pinned rows are the first remaining preview rows. The pane
  is not written back. The preview can show a
  short list of number formats without changing the stored number. Percent
  is built-in format 9 or 10, or a custom format that is exactly `0%` or
  `0.00%`: the number times 100, trimmed the same way other calculated
  numbers are trimmed, then `%`. Thousands is built-in format 3 or exactly
  `#,##0`, and only when the value is a whole number smaller than 1e15 in
  absolute value. A date is built-in format 14 or exactly `yyyy-mm-dd`,
  shown as `yyyy-mm-dd` from the 1900 serial, including the fake 29 February
  1900. A time fraction is dropped. A 1904 workbook is not shifted. Any
  other format, and any cell that is not a number, stays the stored text.
  At most 64 cell formats are read. `TEXT` is still the formula that writes
  a formatted value. Enter or Save writes
  the current cell back into the workbook. A formula
  cell is left unchanged. A value edit recalculates arithmetic, comparisons,
  cell references, `SUM`, `AVERAGE`, `MIN`, `MAX`, `COUNT`, `IF`, `ROUND`,
  `ABS`, `INT`, text joined with `&` or `CONCAT`, `LEN`, `LEFT`, `RIGHT`,
  `MID`, `UPPER`, `LOWER`, `TRIM`, `SUBSTITUTE`, `FIND`, `SEARCH`, `REPT`,
  `EXACT`, `AND`, `OR`, `NOT`, `SQRT`, `POWER`, `MOD`, `SIGN`, `PRODUCT`,
  `QUOTIENT`, `PI`, `EVEN`, `ODD`, `REPLACE`, `VALUE`, `T`, `N`, `ROUNDUP`,
  `ROUNDDOWN`, `CEILING.MATH`, `FLOOR.MATH`, `MEDIAN`, `ISNUMBER`,
  `ISTEXT`, `GCD`, `LCM`, `LN`, `LOG10`, `LOG`, `EXP`, `FACT`, `SIN`, `COS`,
  `TAN`, `RADIANS`, `DEGREES`, `ASIN`, `ACOS`, `ATAN`, `ATAN2`, `SINH`,
  `COSH`, `TANH`, `COMBIN`, `PERMUT`, `PERMUTATIONA`, `LARGE`, `SMALL`,
  `TRUNC`, `ISEVEN`, `ISODD`, `CODE`, `CHAR`, `COUNTA`, `COUNTBLANK`,
  `IFERROR`, `CLEAN`, `PROPER`, `CHOOSE`, `SWITCH`, `XOR`, `TEXTJOIN`,
  `IFS`, `BITAND`, `BITOR`, `BITXOR`, `CEILING`, `FLOOR`, `BITLSHIFT`,
  `BITRSHIFT`, `MROUND`, `SUMIF`, `COUNTIF`, `AVERAGEIF`, `SUMPRODUCT`, `MINIFS`, `MAXIFS`, `SUMIFS`, `AVERAGEIFS`, `SUMSQ`, `STDEV`, `STDEV.S`, `STDEVP`, `STDEV.P`, `VAR`, `VAR.S`, `VARP`, `VAR.P`, `AVEDEV`, `DEVSQ`, `GEOMEAN`, `HARMEAN`, `COUNTIFS`, `SLOPE`, `INTERCEPT`, `CORREL`, `PEARSON`, `RSQ`, `FORECAST`, `FORECAST.LINEAR`, `STEYX`, `COVARIANCE.P`, `COVAR`, `COVARIANCE.S`, `RANK`, `RANK.EQ`, `RANK.AVG`, `PERCENTILE`, `PERCENTILE.INC`, `PERCENTILE.EXC`, `QUARTILE`, `QUARTILE.INC`, `QUARTILE.EXC`, `MODE`, `MODE.SNGL`, `PERCENTRANK`, `PERCENTRANK.INC`, `PERCENTRANK.EXC`, `STANDARDIZE`, `SKEW`, `SKEW.P`, `KURT`, `TRIMMEAN`, `FISHER`, `FISHERINV`, `SQRTPI`, `COMBINA`, `SUMX2MY2`, `SUMX2PY2`, `SUMXMY2`, `GESTEP`, `DELTA`, `MULTINOMIAL`, `FACTDOUBLE`, `POISSON`, `POISSON.DIST`, `BINOM.DIST`, `BINOMDIST`, `EXPON.DIST`, `EXPONDIST`, `NEGBINOM.DIST`, `NEGBINOMDIST`, `HYPGEOM.DIST`, `HYPGEOMDIST`, `WEIBULL.DIST`, `WEIBULL`, `GAMMA`, `GAMMALN`, `GAMMA.DIST`, `GAMMADIST`, `BINOM.INV`, `CRITBINOM`, `CHISQ.DIST`, `CHISQ.DIST.RT`, `CHIDIST`, `NORM.S.DIST`, `NORMSDIST`, `NORM.DIST`, `NORMDIST`, `ERF`, `ERFC`, `GAUSS`, `PHI`, `LOGNORM.DIST`, `LOGNORMDIST`, `BINOM.DIST.RANGE`, `Z.TEST`, `ZTEST`, `PROB`, `ASINH`, `ACOSH`, `ATANH`, `SEC`, `CSC`, `COT`, `CONCATENATE`, `UNICHAR`, `UNICODE`, `SERIESSUM`, `NORM.S.INV`, `NORMSINV`, `NORM.INV`, `NORMINV`, `LOGNORM.INV`, `LOGINV`, `CONFIDENCE`, `CONFIDENCE.NORM`, `GAMMA.INV`, `GAMMAINV`, `CHISQ.INV`, `CHISQ.INV.RT`, `CHIINV`, `ROMAN`, `ARABIC`, `SECH`, `CSCH`, `COTH`, `ACOT`, `ACOTH`, `CHISQ.TEST`, `CHITEST`, `AVERAGEA`, `MINA`, `MAXA`, `STDEVA`, `VARA`, `DATE`, `YEAR`, `MONTH`, `DAY`, `DEC2BIN`, `BIN2DEC`, `DEC2HEX`, `HEX2DEC`, `DEC2OCT`, `OCT2DEC`, `BASE`, `DECIMAL`, `BESSELJ`, `BESSELI`, `MDETERM`, `INDEX`, `MATCH`, `VLOOKUP`, `HLOOKUP`, `BETA.DIST`, `T.DIST`, `T.DIST.RT`, `T.DIST.2T`, `TDIST`, `F.DIST`, `F.DIST.RT`, `FDIST`, `T.TEST`, `TTEST`, `F.TEST`, `FTEST`, `CONFIDENCE.T`, `PMT`, `FV`, `PV`, `NPER`, `RATE`, `NPV`, `IRR`, `EDATE`, `EOMONTH`, `WEEKDAY`, `WORKDAY`, `NETWORKDAYS`, `DATEDIF`, `FREQUENCY`, `LINEST`, `TREND`, `MODE.MULT`, `IPMT`, `PPMT`, `CUMIPMT`, `CUMPRINC`, `XNPV`, `XIRR`, `MIRR`, `TEXT`, `CONVERT`, `PRICE`, `ODDFPRICE`, `ODDLPRICE`, `YIELD`, `ODDFYIELD`, `ODDLYIELD`, `DURATION`, `MDURATION`, `DSUM`, `DAVERAGE`, `DCOUNT`, `DCOUNTA`, `DMIN`, `DMAX`, `SORT`, `SORTBY`, `UNIQUE`, `FILTER`, `XLOOKUP`, `XMATCH`, `LET`, `INDIRECT`, `OFFSET`, `TEXTBEFORE`, `TEXTAFTER`, `TEXTSPLIT`, `GROWTH`, `LOGEST`, `TAKE`, `DROP`, `CHOOSECOLS`, `CHOOSEROWS`, `HSTACK`, `VSTACK`, `SUBTOTAL`, `GOALSEEK`, and `WHATIF` on that sheet.
  Text is stored as an inline string. Length,
  slices, and find positions count Unicode scalar values. `TRIM` collapses
  only U+0020. `SEARCH` ignores ASCII letter case. One `*` `?` `~` pattern
  matches a prefix of the remaining text. `~` escapes the next character.
  A pattern longer than 64 scalar values, or a text longer than 256, leaves
  the stored value. `FIND` stays an exact match. `REPT` stops at 32,767
  scalar values. `EXACT` writes 1 or 0. `AND` and
  `OR` take up to 255 comma-separated numbers, comparisons, or ranges and
  write 1 or 0. One argument may be a range of at most 256 cells. Blank
  cells and text in that range are skipped. A range with no finite number,
  or a larger range, leaves the stored value. Collected numbers stop at 256.
  `XOR` uses those same arguments and writes 1 when an odd count of them is
  not zero. `NOT` stays one value and does not expand a range. `SUBTOTAL`
  takes a function number from 1 through 11 and up to 16 ranges. 1 is
  `AVERAGE`, 2 `COUNT`, 3 `COUNTA`, 4 `MAX`, 5 `MIN`, 6 `PRODUCT`, 7
  `STDEV`, 8 `STDEVP`, 9 `SUM`, 10 `VAR`, and 11 `VARP`. Each range is at
  most 1024 cells and the call stops at 4096 cells. A blank is skipped.
  Text is skipped by the numeric functions and counted by `COUNTA`. A cell
  on the sheet being edited whose formula is `SUBTOTAL` is skipped. Numbers
  101 through 111 are those same functions and skip hidden rows. A row is
  hidden when its `hidden` attribute is 1 or `true`, or when a single
  autofilter column does not match it. The header row stays. The filter has
  at most 8 exact values, ignores ASCII case, and covers at most 400 rows.
  A second column, a custom filter, or a wildcard is ignored. The preview
  omits hidden rows. Nothing is written back.
  An empty `SUM`, `PRODUCT`, `COUNT`, or `COUNTA` writes 0. `TEXTJOIN` takes a
  delimiter, a number, and the same text arguments as `CONCAT`, including
  a cell range. A nonzero number skips empty text. A missing cell inside
  a range is empty text. A shared-string cell contributes its text. A missing cell
  named on its own leaves the stored value. The joined text stops at
  32,767 scalar values. An empty value list writes an empty string. `NOT` turns zero into 1 and any
  other number into 0. `SQRT` of a negative number leaves the stored
  value. `POWER` of a negative base with a non-integer exponent leaves
  the stored value. `MOD` follows the sign of the divisor, and a zero
  divisor leaves the stored value. `SIGN` writes -1, 0, or 1. `PRODUCT`
  multiplies the same numeric arguments as `SUM`; an empty call writes 0.
  `QUOTIENT` drops the fraction toward zero. A zero divisor, or a magnitude
  at or above 1e15, leaves the stored value. `PI` writes the 64-bit
  constant to eight decimal places. `EVEN` and `ODD` round away from zero.
  `REPLACE` removes a 1-based span of Unicode scalar values. A start before
  1, or more than one past the end, leaves the stored value. `VALUE` reads
  an optional sign, digits, and one dot. Thousands separators, exponents,
  and dates leave the stored value. `T` keeps text and writes an empty
  string for a number. `N` keeps a number and writes 0 for text. `ROUNDUP`
  rounds away from zero and `ROUNDDOWN` toward zero, with a digit count
  from -10 through 10. `CEILING.MATH` and `FLOOR.MATH` move toward +infinity
  or -infinity, to a multiple of the significance. An omitted significance is
  1. The sign of the significance is ignored, and a significance of 0 writes
  0. A third number that is not 0 reverses the direction for a negative
  number. A non-finite result or text leaves the stored value. `CEILING` and `FLOOR` take a significance. The
  signs must match. A zero significance makes `CEILING` write 0 and makes
  `FLOOR` leave the stored value. `CEILING` moves away from zero and
  `FLOOR` moves toward zero, to a multiple of that significance. A
  magnitude at or above 1e15, or a call with one argument, leaves the
  stored value. `MEDIAN` uses the same numeric arguments as
  `SUM`, including a cell range. An even count averages the two middle
  numbers. An empty call leaves the stored value. `ISNUMBER` and `ISTEXT`
  write 1 or 0. A missing cell leaves the stored value. A shared-string cell
  is text, so `ISTEXT` writes 1 and `ISNUMBER` writes 0. `GCD` and `LCM` use those same numeric arguments.
  A fraction is dropped toward zero. A negative number, a magnitude at or
  above 1e15, or an empty call leaves the stored value. `LCM` of a zero
  writes 0. `LN` and `LOG10` need a positive number. `LOG` uses base 10
  when the base is omitted. A base that is not positive, or is 1, leaves
  the stored value. `EXP` leaves the stored value when the result is not
  finite. `FACT` drops the fraction toward zero. A negative number, or 171
  and above, leaves the stored value. Above 22 the product is no longer an
  exact integer. `SIN`, `COS`, and `TAN` take radians. `RADIANS` and
  `DEGREES` convert a number. A non-finite result leaves the stored value.
  `ASIN` and `ACOS` need a number from -1 through 1. `ATAN2` takes x then
  y, and leaves the stored value when both are zero. `SINH`, `COSH`, and
  `TANH` leave the stored value when the result is not finite. `COMBIN`
  drops the fraction toward zero. A negative number, a second number
  larger than the first, or a first number at or above one million leaves
  the stored value. While the result is below 1e15 it is rounded to an
  integer. `PERMUT` uses the same limits and does not repeat items.
  `PERMUTATIONA` allows repetition. Zero to a positive power is 0, and zero
  to the power 0 is 1. `LARGE` and `SMALL` use the same numeric arguments
  as `SUM`, and the last argument is a 1-based rank. A rank below 1 or past
  the last number leaves the stored value. `TRUNC` drops the fraction
  toward zero, the same way as `ROUNDDOWN`. The digit count defaults to 0
  and must be from -10 through 10. `ISEVEN` and `ISODD` write 1 or 0. The
  fraction is dropped toward zero. Zero is even. A magnitude at or above
  1e15, or text, leaves the stored value. `CODE` and `CHAR` use Unicode
  scalar values. `CODE` reads the first scalar. A number is shown as text
  first, and an empty text leaves the stored value. `CHAR` drops the
  fraction toward zero. Code 0, a surrogate, or a value above 1114111
  leaves the stored value. `COUNTA` counts stored numbers and non-empty
  text, including a shared string. `COUNTBLANK` counts the rest, including an empty inline
  string. A missing cell inside a range counts as blank, and a missing
  cell named on its own leaves the stored value. An index past the
  shared-string table counts as blank. An empty call writes 0. `IFERROR` takes two arguments.
  When the first cannot be calculated, or is not a finite number, the
  second is written. The second is left unread when the first succeeds.
  A third argument, or a second that also fails, leaves the stored value.
  `CLEAN` removes characters below U+0020. `PROPER` uppercases the first
  Unicode letter of each word and lowercases the rest. A character that is
  not a letter starts a new word. `CHOOSE` takes a 1-based index and up to
  254 values. The index is truncated toward zero. Only the chosen value is
  calculated. An index below 1, an index past the last value, or a chosen
  value that cannot be calculated leaves the stored value. `SWITCH`
  compares one value with up to 126 later values. Numbers match within
  1e-9. Text matches exactly. A number does not match text. The last
  argument is the default when the call has an even count. Only the chosen
  result is calculated. No match and no default, or a chosen result that
  cannot be calculated, leaves the stored value. `IFS` takes up to 127
  condition and value pairs. The first nonzero number selects its value,
  and later pairs are left unread. A text condition, an odd argument count,
  no match, or a chosen value that cannot be calculated leaves the stored
  value. `BITAND`, `BITOR`, and `BITXOR` drop the fraction toward zero. A
  negative number, or a magnitude at or above 2^48, leaves the stored
  value. `BITLSHIFT` and `BITRSHIFT` use that same number. The shift count
  is truncated toward zero, and a negative count shifts the other way. A
  count past 53, or a result at or above 2^48, leaves the stored value.
  `MROUND` rounds to the nearest multiple. A tie moves away from zero. The
  signs must match. A zero multiple leaves the stored value, and `MROUND`
  of zero and zero writes 0. A magnitude at or above 1e15 leaves the stored
  value. `SUMIF` takes one cell range and a criterion. The criterion is a
  number, or text that starts with `=`, `<>`, `>=`, `<=`, `>`, or `<` and
  then a number. Numbers match within 1e-9. Only stored numbers are added.
  No match writes 0. Other text without `*`, `?`, or `~`, a third argument,
  or a call that does not start with a range leaves the stored value. A
  criterion containing `*`, `?`, or `~` matches text. The match ignores
  ASCII case. `~` escapes the next character. `*` and `?` count Unicode
  scalar values. A pattern longer than 64 scalar values, or a cell longer
  than 256, leaves the stored value. A wildcard does not match a number.
  `SUMIF` still adds only numbers, so that criterion writes 0. `COUNTIF`
  counts the text cells that match. `AVERAGEIF` with no matching number
  leaves the stored value. `=c*` and `<>c*` use the same pattern. `>apple`
  still leaves the stored value. `SUMIFS`, `AVERAGEIFS`, `MINIFS`, and
  `MAXIFS` accept that same one wildcard on their criteria range and still
  read numbers from the value range. There is no second criteria pair.
  `COUNTIF` uses that same range and criterion and writes how many stored
  numbers match. A wildcard counts text cells instead. No match writes 0.
  `AVERAGEIF` uses that same range and criterion and writes the average.
  No match leaves the stored value. `SUMPRODUCT` multiplies
  equal-sized cell ranges and adds the products. One range is a sum. A
  blank cell, a text cell, or a shared-string cell counts as 0. Up to 8
  ranges are read. A different size, a ninth range, or an argument that is
  not a range leaves the stored value. `MINIFS` and `MAXIFS` take a value
  range, one criteria range of the same size, and one criterion of the same
  kind. Only stored numbers are considered. No match writes 0. A different
  size, other text without a wildcard, or a second criterion leaves the stored
  value. `SUMIFS` adds those same matching numbers. No match writes 0. A
  different size, other text without a wildcard, or a second criterion leaves
  the stored value. `AVERAGEIFS` writes the average of those same matching
  numbers. No match leaves the stored value. A different size, other text
  without a wildcard, or a second criterion leaves the stored value.
  `SUMSQ` adds the squares of the numbers `SUM` would read. Text inside a
  range is skipped. An empty call writes 0. Other text, or a result that is
  not finite, leaves the stored value.
  `STDEV` and `STDEV.S` write the sample standard deviation, dividing by
  one less than the count. Fewer than two numbers leaves the stored value.
  `STDEVP` and `STDEV.P` divide by the count. One number writes 0. An empty
  call leaves the stored value. Text inside a range is skipped. Other text,
  or a result that is not finite, leaves the stored value.
  `VAR` and `VAR.S` write the sample variance, dividing by one less than
  the count. Fewer than two numbers leaves the stored value. `VARP` and
  `VAR.P` divide by the count. One number writes 0. An empty call leaves
  the stored value. Text inside a range is skipped. Other text, or a
  result that is not finite, leaves the stored value.
  `AVEDEV` writes the average absolute deviation from the mean of the
  numbers `SUM` would read. One number writes 0. An empty call leaves the
  stored value. Text inside a range is skipped. Other text, or a result
  that is not finite, leaves the stored value.
  `DEVSQ` adds the squared deviations from that same mean. One number
  writes 0. An empty call leaves the stored value. Text inside a range is
  skipped. Other text, or a result that is not finite, leaves the stored
  value.
  `GEOMEAN` writes the geometric mean of the numbers `SUM` would read.
  Every number must be positive. One positive number writes that number.
  An empty call, a zero, a negative, other text, or a result that is not
  finite leaves the stored value. Text inside a range is skipped.
  `HARMEAN` writes the harmonic mean of the numbers `SUM` would read.
  Every number must be positive. One positive number writes that number.
  An empty call, a zero, a negative, other text, or a result that is not
  finite leaves the stored value. Text inside a range is skipped.
  `COUNTIFS` counts stored numbers in one range with one criterion, the
  same way `COUNTIF` does, including a wildcard. No match writes 0. Other
  text without a wildcard, a second criterion, or a call that does not
  start with a range leaves the stored value.
  `SLOPE` takes a y range and an x range of the same size. A pair is used
  when both cells hold finite stored numbers. Text, blanks, and shared
  strings are skipped. Fewer than two pairs, a zero spread in x, a different
  size, or an argument that is not a range leaves the stored value.
  `INTERCEPT` uses those same pairs and writes where the line crosses y at
  x = 0. The same failures leave the stored value.
  `CORREL` and `PEARSON` write the correlation of those same pairs. Fewer
  than two pairs, a zero spread in either range, a different size, or an
  argument that is not a range leaves the stored value.
  `RSQ` writes the square of that correlation. A negative line still writes
  a positive square. The same failures leave the stored value.
  `FORECAST` and `FORECAST.LINEAR` take one x and then those same ranges.
  The result is the line at that x. The x must be a finite number. The same
  pair failures leave the stored value.
  `STEYX` writes the standard error of the y values around that line. It
  needs at least three pairs. Fewer than three pairs, a zero spread in x, a
  different size, or an argument that is not a range leaves the stored value.
  `COVARIANCE.P` and `COVAR` divide the paired products by the count.
  `COVARIANCE.S` divides by one less than the count. Fewer than two pairs, a
  different size, or an argument that is not a range leaves the stored value.
  A zero spread writes 0.
  `RANK` and `RANK.EQ` write the rank of one number in one range. Ties
  share the better rank. Order 0, or a missing order, ranks the largest as
  1. Any other finite order ranks the smallest as 1. Text, blanks, and
  shared strings in the range are skipped. Numbers match within 1e-9. A
  missing number, a call without a range, or an extra argument leaves the
  stored value.
  `RANK.AVG` averages the ranks of tied numbers. A single number keeps the
  same rank. The same order, match, and failure rules apply.
  `PERCENTILE` and `PERCENTILE.INC` take one range and a k from 0 through 1.
  k = 0 writes the smallest stored number and k = 1 writes the largest.
  Text, blanks, and shared strings are skipped. An empty range, a k outside
  that span, or a call that does not start with a range leaves the stored
  value.
  `QUARTILE` and `QUARTILE.INC` take that same range and a quartile number.
  The number is truncated toward zero and must land on 0, 1, 2, 3, or 4.
  Those steps are the smallest number, then 0.25, 0.5, 0.75, and the
  largest. The same skips and failures leave the stored value.
  `MODE` and `MODE.SNGL` write the stored number that appears most often in
  one range. A tie writes the number that appears first. Text, blanks, and
  shared strings are skipped. Numbers match within 1e-9. If no number
  repeats, the call is not a range, or there is an extra argument, the
  stored value stays.
  `PERCENTRANK` and `PERCENTRANK.INC` write where one number sits in one
  range, from 0 at the smallest stored number to 1 at the largest. A value
  between two numbers is interpolated. At least two numbers are required.
  Text, blanks, and shared strings are skipped. A number outside the range,
  fewer than two numbers, a call that is not a range, or an extra argument
  leaves the stored value.
  `PERCENTILE.EXC` takes one range and a k strictly between 0 and 1. The
  place is k times one more than the count of stored numbers, and a place
  between two numbers is interpolated. A k of 0 or 1, a place below the
  first number or past the last, an empty range, or a call that is not a
  range leaves the stored value. Text, blanks, and shared strings are
  skipped.
  `QUARTILE.EXC` takes that same range and a quartile number. The number is
  truncated toward zero and must land on 1, 2, or 3. Those steps are the
  exclusive 0.25, 0.5, and 0.75 places. A quartile of 0 or 4, a number
  outside 1 through 3, or a call that is not a range leaves the stored
  value.
  `PERCENTRANK.EXC` writes where one number sits in one range, from one
  divided by one more than the count at the smallest stored number up to
  the count divided by that same total at the largest. It does not write 0
  or 1. A value between two numbers is interpolated. One matching number
  writes 0.5. Text, blanks, and shared strings are skipped. A number
  outside the range, an empty range, a call that is not a range, or an
  extra argument leaves the stored value.
  `STANDARDIZE` writes (x - mean) / scale for three numbers. A scale of 0
  or less, a non-numeric argument, or a missing argument leaves the stored
  value.
  `SKEW` and `SKEW.P` write the skewness of the numbers `SUM` would read.
  Both need at least three numbers. `SKEW` divides by the sample standard
  deviation. `SKEW.P` divides by the population standard deviation. A zero
  spread, fewer than three numbers, an empty call, or direct text leaves
  the stored value. Text inside a range is skipped.
  `KURT` writes the excess kurtosis of those same numbers, using the sample
  standard deviation. At least four numbers are required. A zero spread,
  fewer than four numbers, an empty call, or direct text leaves the stored
  value.
  `TRIMMEAN` takes one range and a fraction from 0 up to, but not including,
  1. It drops that fraction of the stored numbers, the same count from the
  low end and the high end, rounding the dropped count down to an even
  number, then writes the mean of what remains. A fraction of 0 writes the
  mean of every stored number. Text, blanks, and shared strings are skipped.
  A fraction below 0 or of 1 or more, an empty range, or a call that is not
  a range leaves the stored value.
  `FISHER` writes half the natural log of (1 + x) / (1 - x). x must be
  strictly between -1 and 1. `FISHERINV` writes the inverse for any finite
  number. A value on or outside that span, a non-finite result, or text
  leaves the stored value.
  `SQRTPI` writes the square root of a number times pi. A negative number
  or text leaves the stored value.
  `COMBINA` writes how many ways items can be chosen when repeats are
  allowed. Both counts are truncated toward zero and must be at least 0 and
  below 1,000,000. Choosing zero writes 1. Choosing from zero items writes
  0. A negative count, text, or a pair whose formula reaches 1,000,000
  leaves the stored value.
  `SUMX2MY2`, `SUMX2PY2`, and `SUMXMY2` take two ranges of the same length.
  A pair is used only when both cells hold finite stored numbers. Text,
  blanks, and shared strings skip that pair. `SUMX2MY2` adds the first
  square minus the second. `SUMX2PY2` adds the two squares. `SUMXMY2` adds
  the square of the difference. No numeric pairs writes 0. A different size,
  a call that is not two ranges, or a non-finite square leaves the stored
  value.
  `GESTEP` writes 1 when the first number is greater than or equal to the
  second, and 0 otherwise. A missing second number compares with 0. `DELTA`
  writes 1 when the two numbers are exactly equal, and 0 otherwise. A
  missing second number compares with 0. A non-finite number or text leaves
  the stored value.
  `MULTINOMIAL` writes the factorial of the sum divided by the factorial of
  each count. Counts are truncated toward zero and must be at least 0. Their
  sum must stay below 1,000,000. Text inside a range is skipped. An empty
  call, a negative count, direct text, or a sum that reaches 1,000,000
  leaves the stored value.
  `FACTDOUBLE` writes the double factorial. The number is truncated toward
  zero. An even number steps down by two to 2, and an odd number steps down
  by two to 1. Zero and one write 1. A negative number, a number of 301 or
  more, or text leaves the stored value.
  `POISSON` and `POISSON.DIST` take an x, a mean, and a third number. x is
  truncated toward zero. A third number of 0 writes the probability of that
  count. Any other finite third number writes the sum of the probabilities
  from 0 through x. x must be at least 0 and below 171. The mean must be at
  least 0 and below 700. A negative argument, a missing third number, or
  text leaves the stored value.
  `BINOM.DIST` and `BINOMDIST` take a success count, a trial count, a
  probability, and a fourth number. The counts are truncated toward zero. A
  fourth number of 0 writes the probability of that success count. Any other
  finite fourth number writes the sum from 0 through that count. The
  probability must be from 0 through 1. The success count must not pass the
  trial count, and the trial count must be below 171. A negative count, a
  probability outside that span, a missing fourth number, or text leaves the
  stored value.
  `EXPON.DIST` and `EXPONDIST` take an x, a lambda, and a third number. A
  third number of 0 writes lambda times e to the power of minus lambda times
  x. Any other finite third number writes 1 minus that power of e. x must be
  at least 0. Lambda must be greater than 0. A negative x, a lambda of 0 or
  less, a missing third number, an exponent that overflows, or text leaves
  the stored value.
  `NEGBINOM.DIST` takes a failure count, a success count, a probability, and
  a fourth number. A fourth number of 0 writes the probability of that many
  failures before the success count. Any other finite fourth number writes
  the sum from 0 failures through that count. `NEGBINOMDIST` takes the same
  first three numbers and writes that point probability. Both counts are
  truncated toward zero. The failure count must be at least 0 and below 171.
  The success count must be at least 1 and below 171. The probability must be
  greater than 0 and less than 1. A count outside that span, a probability of
  0 or 1, a missing fourth number on `NEGBINOM.DIST`, or text leaves the
  stored value.
  `HYPGEOM.DIST` takes the successes drawn, the draw size, the successes in
  the population, the population size, and a fifth number. A fifth number of
  0 writes the probability of that draw. Any other finite fifth number writes
  the sum from the smallest possible draw through that count. `HYPGEOMDIST`
  takes the same first four numbers and writes that point probability. Every
  count is truncated toward zero, must be at least 0, and must be below 171.
  The draw must fit in the population. The drawn successes must fit the draw
  and the successes available. The rest of the draw must fit the rest of the
  population. A count outside that span, a missing fifth number on
  `HYPGEOM.DIST`, or text leaves the stored value.
  `WEIBULL.DIST` and `WEIBULL` take an x, an alpha, a beta, and a fourth
  number. A fourth number of 0 writes the density. Any other finite fourth
  number writes 1 minus e to the power of minus (x divided by beta) raised
  to alpha. x must be at least 0. Alpha and beta must be greater than 0. A
  density at 0 when alpha is below 1, a non-positive alpha or beta, a
  negative x, a missing fourth number, a power that overflows, or text leaves
  the stored value.
  `GAMMA` writes the gamma function. A positive whole number up to 170 writes
  the factorial of the number one below it. Other finite numbers use a
  Lanczos approximation, and a negative number uses the reflection formula. A
  non-positive whole number, a whole number above 170, a result that overflows,
  or text leaves the stored value. `GAMMALN` writes the natural logarithm of that function for a
  number greater than 0. A number that is not greater than 0, a result that
  overflows, or text leaves the stored value.
  `GAMMA.DIST` and `GAMMADIST` take an x, an alpha, a beta, and a fourth
  number. A fourth number of 0 writes the density. Any other finite fourth
  number writes the cumulative probability. x must be at least 0. Alpha and
  beta must be greater than 0. A density at 0 is 0 when alpha is greater than
  1, and 1 divided by beta when alpha is 1. A density at 0 when alpha is below
  1 leaves the stored value. A whole alpha writes the cumulative probability
  as one minus a Poisson sum through alpha minus 1. That sum is refused when
  alpha is 172 or more, or when x divided by beta is 700 or more. A fractional
  alpha uses a series of at most 200 terms, and a series that does not settle
  leaves the stored value. A negative x, a non-positive alpha or beta, a
  missing fourth number, or text leaves the stored value.
  `BINOM.INV` and `CRITBINOM` take a trial count, a probability, and a
  criterion. They write the smallest success count whose cumulative binomial
  probability is at least the criterion. The trial count is truncated toward
  zero, must be at least 0, and must be below 171. The probability and the
  criterion must be greater than 0 and less than 1. A count outside that span,
  a probability or criterion of 0 or 1, a missing third number, or text leaves
  the stored value.
  `CHISQ.DIST` takes an x, a degree count, and a third number. The degree
  count is truncated toward zero and must be at least 1. A third number of 0
  writes the density of a gamma distribution whose shape is half the degrees
  and whose scale is 2. Any other finite third number writes that cumulative
  probability. `CHISQ.DIST.RT` and `CHIDIST` take an x and a degree count and
  write 1 minus that cumulative probability. An even degree count uses the
  Poisson sum on a cumulative call. That sum is refused when the degrees are
  344 or more, or when x is 1400 or more. An odd degree count uses a series of
  at most 200 terms, and a series that does not settle leaves the stored
  value. A negative x, a degree count below 1, a missing third number on
  `CHISQ.DIST`, or text leaves the stored value.
  `NORM.S.DIST` takes a z and a second number. A second number of 0 writes the
  standard normal density. Any other finite second number writes the cumulative
  probability. `NORMSDIST` takes one number and writes that cumulative
  probability. The cumulative value is one half times one plus or minus the
  lower gamma series for shape 1/2 and z squared over 2, using at most 200
  terms. A series that does not settle leaves the stored value. A density whose
  exponential underflows writes 0. A missing second number on `NORM.S.DIST`, or
  text, leaves the stored value.
  `NORM.DIST` and `NORMDIST` take an x, a mean, a scale, and a fourth number.
  A fourth number of 0 writes the standard normal density of (x minus the
  mean) divided by the scale, then divided by the scale. Any other finite
  fourth number writes the cumulative standard normal probability of that same
  ratio. The scale must be greater than 0. The cumulative call uses the same
  200-term series as `NORM.S.DIST`. A scale that is not greater than 0, a
  series that does not settle, a missing fourth number, or text leaves the
  stored value.
  `ERF` with one number writes the error function from 0 to that number. With
  two numbers it writes the error function of the second minus the error
  function of the first. `ERFC` writes 1 minus the error function. `GAUSS`
  writes the standard normal cumulative probability minus 1/2. `PHI` writes
  the standard normal density. The error function and `GAUSS` use the same
  200-term gamma series as `NORM.S.DIST`. A series that does not settle, or
  text, leaves the stored value.
  `LOGNORM.DIST` takes an x, a mean, a scale, and a fourth number. A fourth
  number of 0 writes the standard normal density of the natural log of x, after
  subtracting the mean and dividing by the scale, then divided by x times the
  scale. Any other finite fourth number writes that cumulative probability.
  `LOGNORMDIST` takes the first three numbers and writes the cumulative
  probability. x and the scale must be greater than 0. The cumulative call uses
  the same 200-term series as `NORM.S.DIST`. An x or scale that is not greater
  than 0, a series that does not settle, a missing fourth number on
  `LOGNORM.DIST`, or text leaves the stored value.
  `BINOM.DIST.RANGE` takes a trial count, a probability, a lower success
  count, and an optional upper count. It writes the sum of the point
  probabilities from the lower count through the upper count. A missing upper
  count uses the lower count. Every count is truncated toward zero. The trial
  count must be at least 0 and below 171. The probability must be from 0
  through 1. The upper count must be at least the lower count, and neither
  count may pass the trial count. A value outside that span, a missing lower
  count, or text leaves the stored value.
  `Z.TEST` and `ZTEST` take one colon range, a target mean, and an optional
  scale. They write 1 minus the standard normal cumulative probability of
  (sample mean minus the target) divided by the scale over the square root of
  the count. A missing scale uses the sample standard deviation. Text and blank
  cells in the range are skipped. The scale must be greater than 0. A call that
  is not a colon range, fewer than two numbers when the scale is missing, a
  scale of 0, a series that does not settle, or text leaves the stored value.
  `PROB` takes a value range, a probability range of the same length, a lower
  bound, and an optional upper bound. It writes the sum of the probabilities
  whose values lie from the lower bound through the upper bound. A missing
  upper bound uses the lower bound, so only equal values match. Text and blank
  cells are skipped on both sides of a pair. No match writes 0. A negative
  probability, an upper bound below the lower bound, ranges of different
  length, a call that is not two colon ranges, or text leaves the stored
  value.
  `ASINH`, `ACOSH`, and `ATANH` write the inverse hyperbolic sine, cosine, and
  tangent. `ASINH` accepts any finite number. `ACOSH` requires a number of at
  least 1. `ATANH` requires a number strictly between -1 and 1. A number
  outside that domain, a non-finite result, or text leaves the stored value.
  `SEC`, `CSC`, and `COT` write the secant, cosecant, and cotangent of a number
  in radians. A zero denominator, a non-finite result, or text leaves the
  stored value.
  `CONCATENATE` joins text the same way `CONCAT` does. `UNICHAR` and
  `UNICODE` are `CHAR` and `CODE`: the same Unicode scalar values, not a
  code page. `UNICODE` reads the first scalar. An empty text, code 0, a
  surrogate, a value above 1114111, or text where a number is required
  leaves the stored value.
  `SERIESSUM` takes a value, a first exponent, a step, and one colon range of
  coefficients. Term i, starting at 0, is that coefficient times the value
  raised to the first exponent plus i times the step. A blank cell counts as
  0, so later powers stay in place. A zero base with a zero exponent writes
  1. Text in the range, a power or term that is not finite, a call that is
  not a colon range, or text leaves the stored value.
  `NORM.S.INV` and `NORMSINV` write the inverse of the standard normal
  cumulative distribution. The probability must be strictly between 0 and 1.
  `NORM.INV` and `NORMINV` take that probability, a mean, and a scale greater
  than 0, and write the mean plus the scale times the standard inverse.
  Probability 0.5 writes 0 for the standard inverse and the mean for the
  scaled inverse. A probability outside that open interval, a scale that is
  not greater than 0, a series that does not settle, or text leaves the
  stored value.
  `LOGNORM.INV` and `LOGINV` write the exponential of the mean plus the scale
  times that standard inverse. The probability must be strictly between 0 and
  1, and the scale must be greater than 0. `CONFIDENCE` and `CONFIDENCE.NORM`
  take an alpha strictly between 0 and 1, a standard deviation greater than
  0, and a sample size. The size drops its fraction toward zero and must be
  at least 1. The result is the standard inverse of one minus alpha over two,
  times the standard deviation, divided by the square root of that size. A
  value outside those limits, a series that does not settle, or text leaves
  the stored value.
  `GAMMA.INV` and `GAMMAINV` search for the value whose gamma cumulative
  probability matches the requested probability. The probability must be
  strictly between 0 and 1, and both the shape and the scale must be greater
  than 0. `CHISQ.INV` does that search for a chi-square cumulative
  probability. Degrees of freedom drop their fraction toward zero and must be
  at least 1. `CHISQ.INV.RT` and `CHIINV` invert the right tail, which is the
  left-tail inverse of one minus the probability. A search that does not pin
  the displayed value,   a shape the gamma cumulative refuses, or text leaves
  the stored value.
  `ROMAN` writes the classic Roman form of a number from 1 through 3999. The
  fraction is dropped toward zero. A form argument other than 0 leaves the
  stored value. `ARABIC` reads that same classic form, ignoring ASCII case,
  and writes the number. A number outside 1 through 3999, a form other than
  0, text that is not that classic form, or text where a number is required
  leaves the stored value.
  `SECH`, `CSCH`, and `COTH` write the hyperbolic secant, cosecant, and
  cotangent. A zero denominator, a non-finite result, or text leaves the
  stored value.
  `ACOT` writes the inverse cotangent in radians, from 0 through pi. Zero
  writes pi over 2, and a negative number adds pi to the arctangent of its
  reciprocal. `ACOTH` writes the inverse hyperbolic cotangent for a number
  whose absolute value is greater than 1. A number on the closed span from -1
  through 1, a non-finite result, or text leaves the stored value.
  `CHISQ.TEST` and `CHITEST` take two colon ranges of the same shape. They
  write the right-tail chi-square probability of the sum of (actual minus
  expected) squared, divided by expected. A single row uses one less than the
  column count as the degrees of freedom. A single column uses one less than
  the row count. A rectangle uses the product of one less than each side. A
  single cell, ranges of different shape, an expected value that is not
  greater than 0, a blank or text cell, a call that is not two colon ranges,
  or a right tail the chi-square helper refuses leaves the stored value.
  `AVERAGEA`, `MINA`, `MAXA`, `STDEVA`, and `VARA` read numbers and count text
  as 0. A blank cell is skipped. A shared string counts as text, so it is 0.
  `STDEVA` and `VARA`
  are the sample standard deviation and variance and need at least two counted
  values. `MINA` and `MAXA` write 0 when nothing is counted. An empty
  `AVERAGEA`, `STDEVA`, or `VARA` call, a non-finite number, or a direct call
  the reader cannot parse leaves the stored value.
  `DATE` writes an Excel 1900 serial number. A year from 0 through 1899 has
  1900 added to it. Month and day overflow into the neighboring months. 29
  February 1900 is serial 60 even though that day did not occur. `YEAR`,
  `MONTH`, and `DAY` read a serial from 0 through 2958465. Serial 0 is
  1900-01-00. A negative serial, a year outside 0 through 9999, a month or day
  whose magnitude reaches 1e9, or text leaves the stored value.
  `DEC2BIN`, `DEC2HEX`, and `DEC2OCT` truncate toward zero and write
  two's-complement text of at most 10 characters. `DEC2BIN` accepts −512
  through 511, `DEC2HEX` accepts −2^39 through 2^39−1, and `DEC2OCT` accepts
  −2^29 through 2^29−1. A negative number always uses 10 characters. An
  optional places argument is an integer from 1 through 10 and pads a
  non-negative result with zeros; a places value other than 10 on a negative
  number, or a places value shorter than the digits, leaves the stored value.
  `BIN2DEC`, `HEX2DEC`, and `OCT2DEC` read at most 10 characters. Hex letters
  ignore ASCII case. A full 10-character string whose high bit is set is
  negative. `BASE` writes an uppercase digit string for a number from 0 up to,
  but not including, 2^53 and a radix from 2 through 36. An optional minimum
  length from 0 through 255 pads with zeros and leaves the stored value when
  it is shorter than the digits. `DECIMAL` reads that same text back when the
  result stays below 2^53 and the text is at most 255 characters. An empty
  string, an invalid digit, or text where a number is required leaves the
  stored value.
  `BESSELJ` and `BESSELI` take the value first and then an order. The order is
  truncated toward zero and must be from 0 through 40. The absolute value must
  stay below 40. The series stops within 200 terms; if it does not settle, the
  stored value stays.
  `MDETERM` reads one square range of at most 10 by 10 on the same sheet. A
  blank cell counts as 0. Text, including a shared string, a range that is not
  square, a side longer than 10, or a non-finite determinant leaves the stored
  value. A pivot whose absolute value is at most 1e-12 makes the determinant 0.
  `INDEX`, `MATCH`, `VLOOKUP`, and `HLOOKUP` read one range on the same sheet.
  A number matches only a number, with `==`. Text ignores ASCII case. An exact
  match accepts `*`, `?`, and `~` in the lookup text. `*` and `?` count Unicode
  scalar values. A pattern longer than 64 scalars, or a cell longer than 256,
  leaves the stored value. Approximate match does not use wildcards. A range is
  read from its top-left cell. `MATCH`
  type 0 is exact. An omitted type, or type 1, walks an ascending row or column
  and keeps the last key that is less than or equal to the lookup, then stops
  at the first greater key. Type −1 does that for a descending range and a key
  greater than or equal to the lookup. `VLOOKUP` and `HLOOKUP` do the ascending
  walk when the range lookup is omitted or is any number other than 0. A range
  lookup of 0 is exact. A blank cell, or a number beside text, stops an
  approximate walk. A shared string is text. `INDEX` row and column numbers are 1-based
  and truncated toward zero. An omitted column is accepted only when the range
  has one column. A row or column of 0, a position past the range, or a value
  that is not found leaves the stored value. A blank cell writes 0 when it is
  the returned cell. A shared string is returned as its text. `MATCH` skips a
  blank on an exact match, so a blank matches neither 0 nor an empty string.
  A shared string can match text.
  `BETA.DIST` reads x, alpha, beta, and a cumulative flag on the unit interval.
  Alpha and beta must be greater than 0. A cumulative flag of 0 writes the
  density; any other finite number writes the regularized incomplete beta. The
  continued fraction stops within 200 steps, and a fraction that does not
  settle leaves the stored value. A fifth or sixth argument, the distribution
  bounds, is not read.
  `T.DIST` reads x, degrees of freedom, and a cumulative flag. Degrees of
  freedom must be greater than 0. A flag of 0 writes the density; any other
  finite number writes the cumulative distribution. `T.DIST.RT` writes the
  right tail. `T.DIST.2T` and `TDIST` require x to be at least 0. `TDIST` tails
  must be 1 or 2. They use the same continued fraction, so a fraction that does
  not settle leaves the stored value.
  `F.DIST` reads x, two degrees of freedom, and a cumulative flag. Both degrees
  of freedom must be greater than 0, and x must be at least 0. A flag of 0
  writes the density; any other finite number writes the cumulative
  distribution. `F.DIST.RT` and `FDIST` write the right tail. They use the same
  continued fraction.
  `T.TEST` and `TTEST` compare two ranges on the same sheet. Tails must be 1 or
  2. The probability uses the absolute statistic: tails 1 is the upper tail,
  and tails 2 is twice that, capped at 1. Type 1 is paired and the ranges must
  have the same length. A pair is kept only when both cells are finite numbers,
  and at least two pairs are required. Type 2 assumes equal variance and type 3
  is the Welch test. Each range then needs at least two finite numbers. Text
  and blank cells are skipped. Any other type, a sample variance of 0, or a
  fraction that does not settle leaves the stored value. `F.TEST` and `FTEST`
  are the two-tailed comparison of those sample variances, twice the smaller
  tail and capped at 1. A sample variance of 0 leaves the stored value.
  `CONFIDENCE.T` reads alpha, a standard deviation, and a sample size. Alpha
  must be greater than 0 and less than 1, the standard deviation must be
  greater than 0, and the size is truncated to an integer from 2 through
  1000000. The critical value is a bisection of at most 80 steps; if it does
  not settle, the stored value stays.
  `PMT`, `FV`, `PV`, and `NPER` use the ordinary annuity. The number of periods
  must be greater than 0 and at most 1e6. A rate of −1 or below leaves the
  stored value. A rate of 0 uses the linear form. An omitted future value or
  present value is 0. An omitted type is 0, a payment at the end of the period;
  any other finite type is a payment at the beginning. `RATE` starts at guess
  0.1 when the guess is omitted and takes at most 40 Newton steps. `IRR` does
  the same on a colon range of 2 through 128 numbers. The first `IRR` value is
  time 0. A blank cell counts as 0. Text, including a shared string, leaves the
  stored value. `NPV`
  discounts the following numbers from period 1, so it does not include a
  time-0 payment. A rate that does not settle, or a non-finite result, leaves
  the stored value. `IPMT` and `PPMT` use that same annuity. The period is
  truncated toward zero and must be from 1 through the number of periods. A
  payment at the beginning makes the first `IPMT` 0. `CUMIPMT` and `CUMPRINC`
  add those payments from the start period through the end period, with a
  future value of 0. The span is at most 10000 periods. `XNPV` and `XIRR` read
  two colon ranges of the same length, 1 through 128 numbers for `XNPV` and 2
  through 128 for `XIRR`. A blank or text cell leaves the stored value. The
  year length is 365. A date before the first date leaves the stored value.
  `XIRR` needs one positive value and one negative value, dates that do not go
  backward, and at most 40 Newton steps from guess 0.1. `MIRR` reads one colon
  range of 2 through 128 numbers, a finance rate, and a reinvest rate. A zero
  is neither a deposit nor a return. A range that is all deposits or all
  returns leaves the stored value. `TEXT` reads a value and a format of at most
  64 characters. Up to three sections, split on a semicolon that is not inside quotes: positive,
  negative, and zero. A fourth section, a `[` condition, or a color code leaves
  the stored value. An empty section writes an empty string. The negative
  section is applied to the absolute value, so a minus sign has to be quoted
  text in that section. With two sections, zero uses the first. A text value is
  accepted only by a single `@` section. `@`
  writes the value as text, and a number is shown the same way a calculated
  number is shown. A number format uses `0`, `#`, one dot, a comma that turns
  on thousands separators, and one `%` that multiplies by 100. Extra integer
  digits are kept. `#` after the dot drops trailing zeros, and `0` keeps them.
  A negative number keeps a leading minus, including `-0.00` when decimal
  places remain. A date format uses `yyyy`, `yy`, `mmmm`, `mmm`, `mm`, `m`,
  `dddd`, `ddd`, `dd`, and `d` on the 1900 serial, with `-`, `/`, `.`, space,
  and `:` between them. `mmmm` and `mmm` are English month names. `dddd` and
  `ddd` are English weekday names, using the same serial as the other date
  tokens and numbering Sunday as 1. A 1904 workbook therefore can disagree
  with `WEEKDAY`. Quoted text is copied through. A format that mixes a date
  or time token with a number token, or a scientific, fraction, or color
  code, leaves the stored value. A clock uses `h`, `hh`, `s`, and `ss`.
  `m` or `mm` is minutes when it follows an hour token or comes just before
  a seconds token, and otherwise it is the month. Hours run from 0 through
  23. The time is the fractional day rounded to the nearest second. A
  fraction that rounds to 24:00:00 is shown as 00:00:00 and the date is not
  rolled forward. `AM/PM`, `A/P`, `AM`, or `PM` switches `h` and `hh` to a
  12-hour clock and writes the meridian in the case of that token. Hour 0
  and hour 12 are written as 12. A date token can sit in the same format.
  Elapsed time is a section whose text outside quotes is exactly `[h]`,
  `[hh]`, `[m]`, `[mm]`, `[h]:mm`, `[hh]:mm`, `[h]:mm:ss`, or `[hh]:mm:ss`.
  The bare tokens write the total hours or minutes, rounded half away from
  zero, and the total can pass 24 hours. `[hh]` and `[mm]` use at least two
  digits. `[h]:mm` and `[hh]:mm` write total hours and minutes, and the
  seconds forms add seconds. `[hh]` in those clocks pads the hours. A
  negative value keeps a leading minus. The absolute serial must be below
  1000000. A color or any other bracket leaves the stored value. `CONVERT`
  reads a number and two unit names. The names are case-sensitive. Length is
  `m`, `cm`, `mm`, `km`, `in`, `ft`, `yd`, and `mi`. Mass is `g`, `kg`, `mg`,
  `lbm`, and `ozm`. Time is `sec`, `mn`, `hr`, `day`, and `yr`, and a year is
  365.25 days. Temperature is `C`, `F`, and `K`. A temperature below absolute
  zero, two units from different groups, or an unknown name leaves the stored
  value. Prefixes on an arbitrary unit are not read.
  `EDATE` and `EOMONTH` move a serial by a whole number of months on that same
  1900 system. The day is kept when the target month has it, and otherwise
  becomes the last real day of that month. 29 February 1900 is not treated as
  the end of February. The serial must be at least 1, and the month shift must
  stay within 120000. `WEEKDAY` return type 1 numbers Sunday as 1, type 2
  numbers Monday as 1, and type 3 numbers Monday as 0. An omitted type is 1.
  Any other type leaves the stored value. `DATEDIF` accepts `Y`, `M`, `D`,
  `MD`, `YM`, and `YD`. An end before the start leaves the stored value.
  `NETWORKDAYS` and `WORKDAY` count Monday through Friday. An optional holiday
  range may hold at most 512 numbers; text in that range is skipped. A weekend
  holiday is not counted twice. The inclusive span must be at most 100000 days,
  and `WORKDAY` moves at most 10000 working days. `WORKDAY.INTL` uses the same
  holiday list. Its weekend is a text of seven `0` or `1` characters, Monday
  first, where `1` is a weekend day. An omitted weekend is Saturday and
  Sunday, the same as `WORKDAY`. A numeric weekend code, a string that is not
  those seven characters, or a string of seven `1`s leaves the stored value.
  When `workbookPr` has
  `date1904="1"` or `date1904="true"`, serial 0 is 1904-01-01. `DATE`, `YEAR`,
  `MONTH`, `DAY`, `EDATE`, `EOMONTH`, `DATEDIF`, and a `TEXT` date format shift
  that serial by 1462 onto the 1900 calendar. `WEEKDAY`, `NETWORKDAYS`,
  `WORKDAY`, and `WORKDAY.INTL` shift by 1461, so the fictional 29 February 1900 does not move
  those weekdays. A date before 1904-01-01 leaves the stored value. A holiday
  before that day is skipped. Without the flag, the 1900 system is unchanged.
  `IF` can return one number or one text value. Both branches are calculated,
  and a failed branch leaves the stored value even when it is not chosen.
  The chosen text is written as an inline string. More than 32,767 scalar
  values leaves the stored value. `IFS` returns the text of the first true
  pair and does not calculate later pairs. A text condition leaves the stored
  value.
  A shared-string cell on the sheet being recalculated is read as its shared
  text. An index past the table is blank.   A reference `Sheet!A1` or
  `'My Sheet'!A1` reads one cell from a single pass. Before the edited sheet is
  recalculated, every sheet is passed once in workbook order. Each formula on
  that pass reads stored values on its own sheet, including the stored value of
  another formula there, and it reads an earlier sheet after that sheet's pass.
  At most 4096 formulas on a sheet are passed. A formula that returns nothing
  keeps its stored value. A spill formula keeps its stored value. The pass is
  not written back into the other sheet's file. The sheet name ignores ASCII
  case, and a quoted name is at most 31 Unicode scalar values. A doubled
  apostrophe inside a quoted name is one apostrophe. An unquoted name is the
  same kind of token as a function name. A qualified reference on the sheet
  being edited reads that pass, so `Budgets!A1` is the pass and a bare `A1`
  still follows a live formula. `INDIRECT` reads one text address in A1 style,
  at most 128 characters. `$` is ignored. A sheet name, including the sheet
  being edited, is a qualified reference and reads that one pass. Without a
  sheet name it is a bare reference on the sheet being edited. A range is
  `A1:B2`. `R1C1`, brackets, and another workbook leave the stored value.
  Used as one value, it must be one cell. It expands where a colon range
  expands, and as a whole argument of `SUM`, `AVERAGE`, `MIN`, `MAX`,
  `COUNT`, `PRODUCT`, `AVERAGEA`, `COUNTA`, and `COUNTBLANK`.
  `OFFSET(reference, rows, cols, height, width)` starts at one cell, a colon
  range, or `INDIRECT`. `rows` and `cols` are truncated and must be from
  -1000000 through 1000000. `height` and `width` default to the reference
  size, are truncated, and must be from 1 through 256. The result is at most
  4096 cells and must stay on the sheet. Used as one value, it must be one
  cell. A result that leaves the sheet leaves the stored value. `TEXTBEFORE`
  and `TEXTAFTER` take text, a literal delimiter, and an optional instance.
  The match is case-sensitive and counts Unicode scalar values. The instance
  defaults to 1, is truncated, and must be from 1 through 16. There is no
  match mode, no wildcard, and no value to use when the delimiter is missing.
  A missing delimiter, an empty delimiter, or text longer than 1024 scalar
  values leaves the stored value. A delimiter longer than 64 scalar values
  does the same. `TEXTSPLIT` is recalculated only when the formula is that
  one call. It takes one column delimiter and writes the pieces downward, at
  most 16, into existing value cells. A second delimiter, more than 16
  pieces, or a blocked cell leaves the stored value. Consecutive delimiters
  keep an empty piece. An expression around `TEXTSPLIT` keeps the stored
  value. An array constant `{1,2;3,4}` is a whole argument of `SUM`,
  `AVERAGE`, `MIN`, `MAX`, `COUNT`, `PRODUCT`, `AVERAGEA`, `COUNTA`, or
  `COUNTBLANK`. A comma starts another column and a semicolon starts another
  row. Each row must have the same number of columns. A value is a plain
  number, with an optional sign, or quoted text. There is no cell reference,
  no nested brace, no blank, and no scientific notation. Text is skipped by
  the numeric functions and counts as 0 for `AVERAGEA`. At most 4096 values.
  A ragged array, or the constant used as one value or as a `SUMIF` range,
  leaves the stored value. A cross-sheet range and another workbook are
  not read. A formula can read `Sheet1:Sheet2!A1` or `Sheet1:Sheet2!A1:B2`.
  The sheets are the inclusive span in workbook order, at most 32 sheets and
  4096 cells. Every sheet in the span is read from that same pass, including
  the sheet being edited. A missing sheet name leaves the stored value. The reference
  expands in `SUM`, `AVERAGE`, `MIN`, `MAX`, `COUNT`, `PRODUCT`, `AVERAGEA`,
  `COUNTA`, `COUNTBLANK`, and wherever a colon range is accepted. `CONCAT`,
  `CONCATENATE`, and `TEXTJOIN` do not expand it. A circular reference is
  recalculated only when `calcPr` has `iterate="1"` or `iterate="true"`.
  `iterateCount` is clamped to 1 through 100 and defaults to 100.
  `iterateDelta` defaults to 0.001. A negative or non-finite delta becomes
  0.001, and a larger delta is cut to 1. A reference that is already being
  calculated reads the previous pass, or the stored number on the first pass,
  or 0 when that cell has no stored number. The last pass is written even when
  the change is still larger than the delta. Without the flag, a cycle leaves
  the stored value. When the flag is on, each round also refreshes every
  sheet's qualified values from stored inputs and the previous round. The
  edited sheet is calculated once more after those rounds, so it can be one
  step ahead. Other sheet files are not written. A bare reference on another
  sheet still reads that sheet's stored inputs. A workbook defined name is one
  cell or one colon range on one sheet in this workbook. The name is ASCII
  case-insensitive, at most 255 characters, and uses letters, digits,
  underscores, and dots. It must not look like a cell address. A name with
  `localSheetId` belongs to the sheet at that zero-based position in workbook
  order. On that sheet it hides a workbook name with the same spelling. On
  every other sheet it is not visible. A `localSheetId` that is not a sheet
  index is ignored. A name on the sheet being edited reads those cells
  the same way a bare reference does, so a formula there is recalculated. A
  name on another sheet reads the stored snapshot and does not follow a
  formula. The name expands when it is a whole argument of `SUM`, `AVERAGE`,
  `MIN`, `MAX`, `COUNT`, `PRODUCT`, `AVERAGEA`, `COUNTA`, or `COUNTBLANK`, and
  wherever a colon range is accepted, including `SUMIF` and `SUMPRODUCT`.
  `CONCAT`, `CONCATENATE`, and `TEXTJOIN` do not expand a name. A multi-cell
  name used as a single value, or inside an expression, leaves the stored
  value. A name may also be one expression of at most 256 characters. Every cell
  reference in it must be fully absolute and written after a sheet name, such
  as `Budgets!$A$1`. Those cells are read from that same one pass. The expression cannot use another name. A
  relative reference, a 3D reference, another workbook, or a structured table
  reference is ignored. A formula name is one value and does not expand as a
  range. A formula can read one column of a table as `Table1[Amount]`. The
  table name and the column name are ASCII case-insensitive. The column is the
  data cells under that header, not the header and not a totals row.
  `headerRowCount` must be missing or 1. `totalsRowCount` must be missing, 0,
  or 1. On the sheet being edited those cells are read live. On another sheet
  they are read from the stored snapshot. The column expands where a colon
  range expands. A column used as one value works only when it has a single
  data cell. `Table1[[#This Row],[Amount]]`, `#All`, `#Headers`, `#Data`, and
  `#Totals` are ignored. A shared formula on the sheet being
  edited is the cell whose `<f t="shared">` contains the formula. Each other
  cell with the same `si` and an empty shared formula uses that formula shifted
  by the row and column distance from the master. A `$` keeps that column or
  row fixed. Text inside quotes is not shifted. A shift that would leave the
  sheet, a missing master, or a formula type other than shared, including an
  array formula, leaves the stored value. Shared formulas on other sheets are
  not expanded. `PRICE`, `YIELD`, `DURATION`, and `MDURATION` use basis 0 through 4.
  Basis 0 is the US 30/360 day count. Basis 1 counts the actual serial days
  in the coupon period. Basis 2 counts those same actual days and divides by
  360 over the frequency. Basis 3 divides by 365 over the frequency. Basis 4
  is European 30/360: a day of 31 becomes 30 on both dates. Frequency is 1,
  2, or 4. Any other basis or frequency leaves the stored value. There is no
  odd coupon inside `PRICE` or `YIELD`. `ODDFPRICE` and `ODDFYIELD` price a
  short first period: issue, then settlement, then the first coupon, then
  maturity, and the first coupon must land on the maturity schedule. A longer
  first period is split into at most 8 quasi-coupon periods. The end-of-month
  schedule is kept when the first coupon is the last day of its month. More
  than 8 quasi-coupon periods leaves the stored value. `ODDLPRICE` and
  `ODDLYIELD` price the last period from the
  last interest date through maturity. That span may be up to two normal
  periods. A yield below 0 leaves the stored value. The published short first
  price is 113.59771747, the published zero-coupon long first price is
  92.79420294, and the published long last price is 99.87828601.
  Settlement on or after
  maturity leaves the stored value. `YIELD` takes at most 40 Newton steps,
  starting from the coupon rate, or from 0.05 when the coupon is 0. A yield
  less than or equal to the negative frequency leaves the stored value.
  `DURATION` and `MDURATION` redeem at 100. `MDURATION` divides the Macaulay
  duration in years by one plus the yield per coupon period. `DSUM`,
  `DAVERAGE`, `DCOUNT`, `DCOUNTA`, `DMIN`, and `DMAX` take a database range
  with a header row, a field, and a criteria range of 2 through 8 rows. The
  field is a header name or a column number starting at 1. The first header
  with that spelling is used. A criteria header must match a database header,
  ignoring ASCII case. A blank criteria cell, or a criteria column whose
  header is blank, matches every row. A number matches that number. Text is an
  exact ASCII case-insensitive match, or a numeric comparison when it begins
  with `=`, `<>`, `<`, `>`, `<=`, or `>=`. A numeric comparison matches only
  numbers. `=` and `<>` with text that is not a number compare text. A text
  criterion may contain `*`, `?`, and `~`. `*` and `?` match text, ignoring
  ASCII case, and count Unicode scalar values. `~` escapes the next
  character. A pattern longer than 64 scalar values, or a cell longer than
  256, leaves the stored value. A wildcard does not match a number. `=` and
  `<>` may carry the same pattern. A text ordering such as `>apple` leaves
  the stored value. Columns of one criteria row are AND. Further criteria
  rows, through 7, are OR. A blank criteria row matches every record. More
  than 7 criteria rows leaves the stored value. `DSUM` of no numbers
  is 0. `DAVERAGE`, `DMIN`, and `DMAX` with no numbers leave the stored value.
  Text in the field column is skipped by the numeric functions and counted by
  `DCOUNTA`. `FREQUENCY`, `LINEST`,
  `TREND`, and `MODE.MULT` are recalculated only when the formula is that one
  call. A larger expression keeps the stored value. `FREQUENCY` takes a data
  range of at most 256 cells and a bin range of 1 through 16 numbers. Bins
  must be non-decreasing. A blank in the data is skipped. Text, including a
  shared string, leaves the stored value. A blank or text bin, or a bin that
  falls, leaves the stored value. The first count is the formula cell. Later
  counts go down into existing value cells. A missing cell, a formula, or a
  text cell stops the rest, and those cells keep their previous values. The
  last count is everything above the last bin. `MODE.MULT` uses one range of
  at most 256 cells. Text, blanks, and shared strings are skipped. Numbers
  match within 1e-9. Every value that ties for the highest count, when that
  count is at least 2, is written downward in the order it first appears.
  More than 16 modes, or no repeated number, leaves the stored value. The
  same stop rule applies. `LINEST` takes one column or row of y values and one
  column or row of x values, the same length, at most 256 cells. A blank or
  text pair is skipped. At least two finite pairs are required. The slope is
  written in the formula cell and the intercept in the cell to the right, when
  that cell exists and is a value cell. Otherwise nothing is written. There is
  A constant of 0, which would force the line through the origin, leaves the
  stored value. Any other finite constant keeps the intercept. A statistics
  argument of 0, or no statistics argument, keeps the two-cell result. Any
  other finite statistics argument writes a 5 by 2 grid: slope and intercept,
  their standard errors, r² and the standard error of y, F and degrees of
  freedom, then the regression and residual sums of squares. That grid needs
  at least three finite pairs. A perfect fit, where the residual sum of squares
  is 0, leaves the stored value. Every cell in the grid must already exist and
  be a value cell, or nothing is written. There is still one x variable. `TREND`
  takes those same y and x ranges and a new-x range of 1 through 16 numbers in
  one column or row. Each new x must be a finite number. Predictions are
  intercept plus slope times x, first in the formula cell and the rest
  downward. If any of those cells is missing, a formula, or text, nothing is
  written. `GROWTH` and `LOGEST` use that same one x, and every y that is used
  must be greater than 0. `GROWTH` writes predictions the same way `TREND`
  does: the exp of the intercept plus the slope times x, where the line is fit
  to ln(y). A fourth argument leaves the stored value. `LOGEST` writes m and
  b, the exp of that slope and intercept, the same way `LINEST` writes its two
  cells. A statistics argument other than 0 writes the `LINEST` grid of ln(y),
  with the first row replaced by m and b. A constant of 0, a perfect fit on
  that grid, or a non-positive y leaves the stored value. There is no
  `FORECAST.ETS`. `SORT`, `UNIQUE`, and `FILTER` are recalculated only when the formula
  is that one call. A one-column range is at most 256 cells. A blank is
  skipped. Text leaves the stored value, except an all-text `SORT` column.
  Results
  are written into existing value cells, and if any of those cells is
  missing, a formula, or text, nothing is written. `SORT` order defaults to 1.
  An order of -1 reverses it. Any other order leaves the stored value.
  `UNIQUE` stays one column and keeps the first number, treating values within
  1e-9 as the same. `FILTER` takes a second column of the same height and one
  numeric criterion, the same kind `SUMIF` accepts. A row is kept when that
  criterion matches. No remaining number leaves the stored value. A rectangle
  is at most 256 rows, 16 columns, and 256 cells, and every cell must be a
  finite number. A blank leaves the stored value. Text leaves the stored
  value unless that rectangle has exactly one text column and every other
  cell is a finite number. `SORT` then takes a
  1-based column index and an optional order of 1 or -1. The omitted index is
  the first column. A negative index, 0, or an index past the rectangle leaves
  the stored value. Equal keys keep their original order. `FILTER` keeps that
  one criterion and writes each kept row across. A second criterion leaves the
  stored value. `UNIQUE` does not take a rectangle. `SORT` of one text column
  writes that text downward, ignoring ASCII case. A blank is skipped. A number
  in that column leaves the stored value. `SORT` of that mixed rectangle
  writes each reordered row, including the text. When the sort key is the
  text column, the comparison ignores ASCII case. A numeric key sorts by
  the number. A second text column, a column that mixes text and numbers,
  or a blank leaves the stored value. `SORTBY` still does not write its
  key. `SORTBY(block, key, order)` sorts a
  numeric block by one text column of the same height. The order is 1 or -1
  and defaults to 1. A blank or a number in the key leaves the stored value.
  There is no second key. `FILTER` can test that same text column with one
  text criterion, including one `*` `?` `~` pattern, the same way `SUMIF`
  does. The rows it writes stay numeric. `TAKE`, `DROP`, and `CHOOSECOLS` are recalculated only when the formula
  is that one call. The source is one block of numbers, at most 256 rows
  and 16 columns, and every cell must be a finite number. A blank or text
  leaves the stored value. Counts are truncated and must be from -256
  through 256. `TAKE` keeps that many rows, and an optional second count
  keeps that many columns. A count of 0, or a count larger than that side,
  leaves the stored value. A positive count starts at the beginning and a
  negative count starts at the end. Omitted columns means every column.
  `DROP` removes that many rows, and an optional second count removes that
  many columns. A count of 0 keeps that side. A count that leaves nothing
  leaves the stored value. `CHOOSECOLS` takes one 1-based column index. A
  negative index counts from the end. A second index, 0, or an index past
  the block leaves the stored value. The result is at most 256 cells and is
  written into existing value cells. If any of those cells is missing, a
  formula, or text, nothing is written. An expression around the call leaves
  the stored value. `CHOOSEROWS` takes one 1-based row index on that same
  block. A negative index counts from the end. A second index, 0, or an
  index past the block leaves the stored value. The chosen row is written
  across existing value cells. `HSTACK` and `VSTACK` take 2 through 8 blocks
  of those same numbers. `HSTACK` requires equal heights and writes the
  blocks side by side, at most 16 columns. `VSTACK` requires equal widths
  and writes the blocks one under another, at most 256 rows. A different
  size, a blank, or text leaves the stored value. The result is at most 256
  cells. `GOALSEEK(formula_cell, input_cell, target)` writes the input that
  makes the formula cell equal the target. Both cells are on the sheet being
  edited and must be different. The formula cell must already contain a
  formula. The search starts at that input's stored number, or at 0 when it
  is blank, and takes at most 40 steps. A text target, a missing formula, the
  same cell twice, a result that is not a finite number, or a search that does
  not settle leaves the stored value. The input cell's stored value is not
  changed. This is not Excel Solver. `WHATIF(formula_cell, input_cell, inputs)` is recalculated only when the
  formula is that one call. Both cells are on the sheet being edited, and
  they must be different. The formula cell must already contain a formula.
  The inputs are one column of 1 through 16 finite numbers. A blank, text,
  or a second column leaves the stored value. Each input replaces the input
  cell, and the formula cell is calculated with that number. Only a finite
  number is kept. Text, a failed formula, or a cycle leaves the stored
  value. Results are written downward into existing value cells, and if any
  of those cells is missing, a formula, or text, nothing is written. This is
  not an Excel data table. A second input is
  `WHATIF(formula_cell, row_input, row_values, col_input, col_values)`. The
  row values are one column of 1 through 8 numbers and the column values are
  one row of 1 through 8 numbers. The result is at most 64 cells, written
  across and then down. The two inputs must differ from each other and from
  the formula cell. An expression
  around the call leaves the stored value. `XLOOKUP` and `XMATCH` take one
  row or one column of at most 256 cells. An omitted mode, or mode 0, is an
  exact ASCII case-insensitive match. A lookup value may contain one `*`,
  `?`, and `~` pattern. `~` escapes the next character. A pattern longer than
  64 scalar values, or a key longer than 256, is not a match. Mode 1 returns
  the last key that is not greater than the lookup and stops at the first
  greater key, a blank, or a different type, so the keys should be ascending.
  Mode -1 stops at the first smaller key, so the keys should be descending.
  A wildcard with mode 1 or -1, or any other mode, leaves the stored value.
  Blank cells are skipped on an exact match. The first exact match wins. The
  `XLOOKUP` return range must be the same length. A blank return cell is 0.
  The optional fourth argument is written when nothing matches. A fifth
  argument is the match mode. Without the fourth argument, or when `XMATCH`
  finds nothing, the stored value remains. `XLOOKUP` can also return one
  row when the formula is that one call. The keys are one column. The return
  range has the same number of rows and 2 through 16 columns, and every value
  in the matched row is a number. That row is written across existing value
  cells. A text cell in the matched row, or an expression around the call,
  leaves the stored value. A one-column return stays a single value.
  `XLOOKUP` can also return one column when the keys are one row of at least
  two cells. The return range has the same number of columns and 2 through
  16 rows, and every value in the matched column is a number. That column is
  written downward into existing value cells. A text cell in the matched
  column leaves the stored value. A one-row return stays a single value.
  On a miss, a numeric fourth argument is written into
  the formula cell only, and the cells beside or below it stay as they were.
  A text fourth argument, or no fourth argument, leaves the stored value.
  `LET` binds up to eight names, then calculates the last argument. A name
  starts with a letter or underscore and uses letters, digits, underscores, and
  dots. It must not look like a cell address. A later binding replaces an
  earlier one with the same spelling. Names from the workbook are not visible
  inside `LET`. Cell references and functions still are. There is no `LAMBDA`.
  Other formulas keep their stored
  value. Drawings are copied through.
  PowerPoint stays an HTML card preview.
- **PDF forms:** combo boxes and list boxes accept a listed option label
  from the Form bar. Free text and signatures stay unchanged, and a
  compressed choice field is left as it was.
- **Audit log:** Settings → Policy shows the last 12 lines of `audit.log`.
  The file stays on this computer.
- **Policy locks:** `policy.toml` can also mark appearance, date and time,
  privacy, the terminal grid, input, photos, and the agent read-only in
  Settings. Shortcuts and Marketplace stay editable.
- **Face boxes:** the image viewer draws the rectangles stored in
  `photo-faces.json` on the open picture. A rotated or flipped view hides
  them. The boxes do not name the person.
- **Alacritty grid:** the optional grid draws Sixel and direct Kitty images,
  copies text from OSC 52, and stores the OSC 7 directory. zlib Kitty
  payloads and OSC 52 paste stay ignored.
- **Text mode:** `orchid --tui` lists one local folder in the terminal and
  previews text files up to 256 KiB. It does not start the desktop window,
  network mounts, or the viewers. Labels follow the configured language.
- **Leader map:** Settings → Shortcuts edits each leader letter as a
  command id. Clearing a row removes it. A new row accepts
  `letter=command-id`.
- **Search settings:** Settings → Search edits index roots, exclusion
  patterns, the size limit, and the text and PDF extraction switches.
  Those apply to the running index. The ONNX path is read when the index
  opens. The same fields can be locked from `policy.toml`.
- **Policy:** `policy.toml` beside `config.toml` marks a fixed set of
  settings read-only in Settings. An optional https address in `[policy]`
  is read at startup into that same file. A failed read leaves the previous
  file. `audit.log` in the config directory records policy apply, update
  checks, and shell changes, and is not uploaded.
- **Sign-in shell:** Settings → Shell can open Orchid instead of Explorer
  at the next sign-in for this Windows user. The previous per-user `Shell`
  value is remembered and written back when the switch is off. An empty
  remembered value deletes it, so the machine default returns. `orchid.exe
  --restore-shell` does that without opening the window. The machine-wide
  shell is not changed.
- **Marketplace:** Settings installs the Ink, Dawn, Pine, and Ember palettes
  into the themes folder and can add a built-in widget to the workspace.
  Widget code is not downloaded.
- **Updates and telemetry:** startup checks the latest GitHub release and
  notifies when a newer tag exists. Check for updates opens that page.
  Orchid does not install the download. Telemetry is off by default; when
  on, an anonymous app start (version, OS family, language) is appended
  locally and posted only to an https endpoint.
- **Pen and palm:** Settings shows haptic feedback, palm rejection, and pen
  double-tap. A finger is ignored while a pen is down. Double-tap toggles
  whether the pen drives edge gestures, or holds it in erase. Haptic feedback
  keeps Windows touch and pen tap feedback on the window.
- **Agent:** Settings → Agent talks to Ollama or an OpenAI-compatible
  chat API. Universal Search sends one question that starts with `?`
  on the background job queue and posts the reply as a notification.
  The agent is off until enabled. The API key is DPAPI-wrapped on save.
  Redirects are not followed. There is no tool use and no remembered chat.
- **Search embeddings:** hybrid file search in the desktop app runs a
  compiled-in quantized ONNX model (`orchid.onnx.hash.q.v1`, 64-d).
  Builds without the `ort` feature stay on the synonym stub. A
  `[search].sentence-model` path replaces the graph when it accepts
  `features` and returns `embedding`. A stored `.orchid` vector is reused
  only when its model id matches. The ANN file follows that id
  (`ann.stub.v1` for the stub).
- In-app **window manager**: undock / dock widgets, floating placement, z-order,
  minimize / maximize / restore, in-app taskbar, Ctrl+Tab cycle, edge snap,
  schema v2 persistence.
- Widget **groups** (tab stacks), workspaces, 16×10 layout grid, catalog, dock,
  command palette, leader-key mode, onboarding tour, hint mode (`Win+?`).
- User-remappable shortcuts in Settings, with **Orchid (Commander)**,
  **Windows**, **macOS**, and **Linux** profiles (`[shortcuts].profile` plus
  per-command overrides).
- Nine bundled themes + JSON theme loader; 11 Fluent locales with RTL (ar-SA).

#### File manager & storage
- **Photos:** Files → Photos groups hierarchical tags (`people/Name`,
  `event/Date/Title`, `album/Name`) into people, events, a tag tree, and
  smart albums. Settings → Photos can tag images from `People` and `Events`
  folder names, and can ask Windows for face rectangles in the open folder.
  A face with no person name is tagged `people/unnamed`. The detector does
  not say who someone is. Both switches are off by default.
- **Managed-folder ingest:** chunk files are block-cloned from the source
  when the volume can share extents, and copied otherwise. Two whole files
  with the same content become one hard link.
- **Cloud sign-in:** Files → Connect cloud… opens rclone's browser flow for
  Google Drive, personal OneDrive, and Dropbox, then bookmarks the remote.
  The token stays in rclone's config.
- Dual-pane FM with icons / list / details / gallery, tabs, breadcrumbs,
  drag-and-drop (including OS drop and FM→viewer), tags, colour labels, quick
  filter, virtual folders (Recent, Starred, Tags, Photos, Search results,
  Recycle Bin, categories, network). Browse the Recycle Bin, restore items, permanently
  delete selected items, or empty the bin.
- Find files (`Alt+F7` / Tools): name / mask / regex, size, date, attributes,
  content grep (literal or regex), case sensitivity, archives, indexed search
  (Windows Search then Tantivy), EXIF / IPTC / XMP (`Canon` or `Make=Canon`),
  GPS radius (`lat,lon,km`), save results as a virtual folder; find
  duplicates by BLAKE3 content hash and find large files.
- Toolbar **visited folders** menu: top 5 most frequent paths, then recent;
  persisted with the widget.
- Address bar switches to an editable path with folder autocomplete; focus
  loss restores breadcrumb buttons.
- File/folder context menu shows name, type, size, modified, and MIME at the
  top instead of a separate Properties item.
- Selection: long-press or marquee enters tap-to-toggle mode; the toolbar
  shows **Deselect** only when multiple items are selected; tap empty or
  Escape clears the set; Shift+click range and Ctrl+click still work;
  invert (`*`), name mask (`+` / `-`), files-only / folders-only, attribute
  filter; status bar shows selected size.
- Navigation: Ctrl+PgUp goes up, Alt+F1 / toolbar drive menu switches
  volumes, Ctrl+Shift+T opens the selection in a new tab, Ctrl+Shift+Enter
  opens it in the other pane, Ctrl+B flattens nested files (branch view).
- File operations: F5/F6 copy/move to the other pane (clipboard in
  single-pane), F7 new folder, Shift+F4 new file, F8/Del delete (to Recycle
  Bin; restore / empty from the Recycle Bin folder), Shift+Del
  permanent delete, Ctrl+X cut. Ctrl+Z / Ctrl+Y undo and redo copy, move,
  rename, new file/folder, and Recycle Bin delete (session-only; overwrites
  and permanent deletes are not stacked). Overwrite / Skip / Rename / Overwrite older
  / Resume conflict dialog with “apply to all”. Copy queue with pause,
  resume, cancel, verify, new/changed-only, folder structure only, NTFS
  ADS, timestamps and attributes. Batch rename by pattern; symlink /
  hardlink / junction. Touch action bar when single-click open is on.
  Advanced tools: folder compare / sync / merge, byte-level file compare,
  split / join, checksums (MD5 / SHA-1 / SHA-256 / BLAKE3 / CRC32) with
  sidecar verify, Base64 / UUE encode–decode, bulk attributes / timestamps
  / name case, chmod (and Unix chown), Windows ACL via icacls. Properties
  report (`Alt+Enter`) with EXIF, ID3, Office core metadata (editable),
  Windows Authenticode / PE certificate-table inspection, a Sharing
  section (SMB name, UNC, share / unshare, Windows Sharing tab), and
  Previous Versions (Volume Shadow Copy list, restore, copy beside,
  Windows Previous Versions tab), and BitLocker (volume status, lock /
  unlock with password or recovery key, Windows BitLocker panel).
- System clipboard file copy/paste (`CF_HDROP` + Preferred DropEffect) so
  Ctrl+C / Ctrl+X / Ctrl+V exchange files with Explorer and other apps.
- Browse archives as folders (`archive:`), extract all or selected, create /
  add / delete / test; password, multi-volume, and SFX via 7-Zip. Formats:
  ZIP, RAR, 7z, TAR / TAR.GZ / TAR.XZ / TAR.BZ2, CAB, ISO, ACE, ARJ, LZH;
  nested archives open after a temp extract.
- Encrypted folders (age), managed folders with content-addressed ingest
  (BLAKE3 + FastCDC), rclone network mounts (SFTP / SCP / SMB / FTP / FTPS /
  WebDAV / S3 and OAuth clouds via `rclone-remote`), remote-to-remote copy,
  FTP resume retries, `rclone sync` cloud sync, runtime network bookmarks,
  and letterless Windows UNC mappings.
- Tantivy search with incremental FS watcher, PDF/text/DOCX extractors,
  universal search (files + commands + settings).

#### Viewers
- **Optional Alacritty grid:** Settings → Terminal, or `[terminal].grid = "alacritty"`, opens new sessions on `alacritty_terminal`. The built-in grid stays the default. Images, OSC 52 copy, and OSC 7 on the Alacritty grid are listed above.
- **Terminal inline graphics:** Sixel and Kitty direct images (PNG, 24-bit RGB,
  32-bit RGBA) are drawn in the terminal pane. zlib Kitty payloads are skipped.
- **HDR / OpenEXR display:** scene-linear `.hdr` and `.exr` pixels are
  tone-mapped into the 8-bit image buffer so highlights above 1.0 stay
  visible. The status line marks them tone-mapped.
- **PDF AcroForm fill-in:** the Form bar lists existing fields and writes
  `Name=value` into text boxes, checkboxes, and radio buttons. The page
  reloads in place, or a sibling `*-form.pdf` is written when the file is
  not writable.
- **Spreadsheet and slide preview:** `.xlsx` / `.xlsm` open as tables and
  `.pptx` / `.pptm` / `.ppsx` open as slide cards (speaker notes included).
  `.xlsb` still opens in the archive browser.
- **Browser widget:** catalog **Browser** with an address bar, tabs, Back /
  Forward / Home / Reload (Stop while loading), bookmarks, find-in-page, a
  homepage setting, zoom (Ctrl++/−/0, Ctrl+wheel), drag-reorder tabs, tab
  favicons, downloads (user Downloads folder with progress, Open, Show in
  Files, Ctrl+J), and an embedded WebView2 overlay. `target=_blank` /
  `window.open` open a new tab; middle-click closes a tab; Ctrl+Shift+T
  reopens the last closed tab; Ctrl+Tab / Ctrl+PageDown (and Shift for
  reverse) cycle tabs; Ctrl+1–8 / Ctrl+9 jump to a tab. Empty input opens a
  blank page; a host name navigates with HTTPS; anything else searches
  DuckDuckGo. Missing WebView2 Runtime shows a hint plus Open in the system
  browser.
- **HTML viewer:** embedded WebView2 preview (back / forward / reload,
  source toggle) for local `html`/`htm`/`xhtml` files; falls back to the
  source pane plus Open in the system browser when the WebView2 Runtime
  is missing. Remote HTML uses an in-memory preview (relative assets may
  not resolve).
- **PDF viewer:** outline sidebar, in-page find, drag-select/copy,
  print, and highlight export to a sibling `hl.pdf`.
- **Media viewer (libmpv):** in-app audio/video playback with play/pause,
  seek scrubber, volume/mute, speed, folder playlist, audio-track cycle,
  chapters, embedded and sidecar `.srt`/`.ass` subtitles, album cover art
  (ID3 APIC / folder `cover.jpg`), A-B loop (Shift+A/B, Shift+L clear),
  resume position, manual subtitle open (Ctrl+S) with scale `{`/`}` and
  Alt+↑/↓ position, playlist shuffle (Y) / random jump (R), Windows SMTC
  publish with cover thumbnail (media keys / lock screen), simple EQ
  presets (E: Flat/Bass/Treble/Vocal), hardware decode via `hwdec=auto-copy`
  (H cycles auto-copy ↔ software; status chip shows active decoder), persisted
  volume/mute (`media_prefs.json`), SW blit up to 1920px wide, OSD flash for
  volume/seek/speed/mute, folder auto-advance on EOF (L toggles loop), stop
  that seeks to start and pauses (Backspace), Home/End playlist ends, frame
  step (`,` / `.`), screenshot PNG (Ctrl+Shift+S), sleep timer (T:
  15/30/60/90/off), aspect cycle (V), rotate (Ctrl+R), audio delay
  (Ctrl+[/]), wheel seek and double-click fullscreen on the surface,
  and keyboard shortcuts. Side playlist panel (Q toggle; click row to jump).
  Drop files from Explorer onto a viewer to open them there. SW blit scales
  to the widget viewport (capped at 1920×1080). Video transport chrome
  auto-hides after ~2.5s idle (mouse move / keys / hover reveal; audio-only
  keeps the bar).   Pitch-preserving speed via mpv `audio-pitch-correction`
  / scaletempo2 (K or click speed chip to toggle; persisted). Paused idle
  skips UI republish / slows mpv polling; catalog picker remembers the last
  media folder. Subtitle style presets (G: outline / yellow / box / cyan;
  Ctrl+0 resets). ReplayGain off/track/album (U). Shift+F kiosk, Esc exits
  immersive, Shift+M next monitor. Catalog **Media Player** launcher opens a
  file picker then places a viewer on the canvas (distinct from **Now Playing**
  SMTC). Bundle `mpv-1.dll` / `libmpv-2.dll` under `third-party/mpv/win-x64/`
  (see `docs/BUILDING.md`); without it, chrome remains and files can still
  open in the system player. Subtitle style and ReplayGain choices persist;
  playlist panel open state and SW blit width account for the side list.
  Chapters popup (click chapter chip), volume scrubber (drag / double-click
  mute), clickable EQ and audio-track chips.
- Image, PDF (pdfium), syntax-highlighted text (Tree-sitter),
  archives (ZIP / 7z / TAR / TAR.GZ / TAR.XZ).
- Image display: fit-to-window / width / height, 1:1, shrink-if-larger
  (no upscale), theme / black / white / gray / custom / checkerboard
  backgrounds (alpha), EXIF orientation auto-rotate, ICC color management
  toward the Windows monitor profile (or sRGB). Fullscreen (F11),
  borderless kiosk, and next-monitor (M). Slint stays 8-bit RGBA; Radiance
  HDR and OpenEXR are tone-mapped into that buffer.
- Image folder navigation: next/prev/first/last, go-to-N, random, loop
  at the ends, skip unreadable files, recently viewed list, and jump to
  the folder in the file manager. PgUp/PgDn / Space / arrows (when
  fitted), mouse wheel, and horizontal swipe.
- Image zoom / pan: percent field, zoom-to-selection (Shift+drag),
  cursor-anchored Ctrl+wheel, pinch + two-finger pan, magnifier (Z),
  thumbnail overview for large images, and restore last zoom when
  switching files.
- Image view-only transforms: 90° CW/CCW, 180°, free angle (field or
  `[` / `]`), horizontal / vertical flip, and reset orientation (no
  file write).
- Lossless JPEG/PNG file transforms: rotate 90/180/270, flip, MCU-aligned
  JPEG crop, EXIF auto-rotate without recompress, and batch rotate of
  the folder playlist.
- Image thumbnail strip (top/bottom), folder grid mode, S/M/L size,
  name/size/date/rating under each cell, preload of the next N images,
  shared on-disk thumbnail cache (mtime-keyed; refreshes when the file
  changes), fast EXIF/embedded-JPEG thumbs, and a contact-sheet PNG
  (`T`/`G`/`D`/`I`/`P`).
- Image folder browse: Timeline (EXIF/mtime), Map (GPS pins), and Calendar
  month grid (`Ctrl+Shift+T` / `M` / `C`, Esc). **Files → Photos** can
  mark detected faces as `people/unnamed`. The viewer does not draw boxes.
- Image chrome auto-hides after idle time (tap or vertical swipe to
  peek); mouse swipe / right-drag next-prev, double-click fit/actual;
  touch pinch, two-finger pan, swipe, tap, and double-tap.
- Image slideshow: auto-advance every N seconds (F5), pause/resume (Space),
  speed (`,`/`.`), random order (Y), fade/slide/dissolve/wipe with adjustable
  duration (J / Shift+J), loop, name/date/EXIF overlay (Shift+O), folder
  background music (Shift+M / ffplay), and export to HTML player, ffmpeg
  MP4, optional self-running EXE, and `.scr` screensaver.
- Image metadata inspector: EXIF (camera / lens / exposure / date), IPTC,
  XMP, GPS with OpenStreetMap, file size / dimensions / bit depth / ICC,
  MD5 and SHA-256, brightness+RGB histogram, and RGB/HSL/HEX/CMYK under
  the cursor. Shift+I opens the panel; Ctrl+Shift+O overlays EXIF;
  Ctrl+H cycles the histogram (`Shift+I` / `Ctrl+Shift+O` / `Ctrl+H`).
- Image metadata editing: IPTC / XMP fields, set or clear GPS, set or shift
  the shoot date, strip all tags or GPS only (privacy), copy tags between
  files, CSV/XML export and CSV import, and folder templates
  (`orchid-meta-templates.json`). File Manager **Tools → Metadata**; the
  viewer panel can save, strip, and export the open file.
- Destructive image edits write a sibling file (never the original):
  rectangle crop with optional aspect lock (1:1 / 4:3 / 3:2 / 16:9) and
  keep-width / keep-height, resize to `%` / `px` / `cm` with
  Nearest / Bilinear / Bicubic / Lanczos, canvas expand, perspective
  (four corners), straighten along a line, and auto-straighten.
  Viewer toolbar (`Shift+C` crop copy); FM **Tools → Image edit** for
  batch resize / canvas / auto-straighten.
- Image tone / color corrections write a sibling file: brightness /
  contrast, exposure / highlights / shadows, auto-levels / auto-contrast /
  auto-color (gray-world), white-balance temperature / tint, saturation /
  vibrance / hue / gamma, packed curves and levels, selective color,
  channel mixer, grayscale / sepia / invert, posterize / solarize /
  threshold. Viewer panel (`Shift+L` / ☼); FM **Tools → Image edit**.
- Image filters write a sibling file: sharpen / unsharp mask, Gaussian and
  motion blur, median / despeckle, emboss, edge detect, oil / watercolor /
  cartoon / pencil sketch, grain, vignette, barrel-pincushion and chromatic
  aberration correction, red-eye, skin smoothing, stacked recipes, and
  one-click looks (`vivid` / `soft` / `drama` / `clean` / `fade`) plus
  folder presets (`orchid-filter-presets.json`). Viewer panel (`Shift+F` /
  ✻); FM **Tools → Image edit**.
- Image annotations write a sibling file: line / arrow, rectangle / ellipse /
  polygon, freehand pen, text (font / size / color), callouts, privacy
  blur or pixelate, highlight, text and image watermarks with nine-slot
  placement and opacity, batch watermark, and a shoot-date stamp.
  Viewer panel (`Shift+D` / ✎); FM **Tools → Image edit**.
- Multi-image tools in FM **Tools → Image edit**: batch convert (JPEG / PNG /
  WebP / BMP), lossless rotate, thumbnail export, image rename templates
  (`{date}` / `{w}` / `{h}`), recipe preview / cancel / save
  (`orchid-batch-recipes.json`), 2–4 image compare and pick-best, pixel
  diff, composite / merge, panorama stitch, and HDR merge from brackets.
  Existing batch resize / adjust / watermark / metadata stay in the same menu.
- Image print: single photo or batch, paper size and margins, 2/4/6/9-up,
  index / contact sheet, on-screen preview, header/footer metadata tokens,
  and ICC destination (`srgb` / monitor / `.icc` file). Viewer panel
  (`Shift+P` / ⌨ Ctrl+P / ⎙); FM **Tools → Image edit**.
- Image formats: JPEG, PNG, GIF, BMP, TIFF, WebP, TGA, ICO/CUR, PNM
  (PBM/PGM/PPM), DDS, Radiance HDR, OpenEXR, plus JPEG-XL, Photoshop PSD,
  GIMP XCF, and PCX. Vector: SVG/SVGZ (`resvg`), Illustrator AI (Pdfium
  first page or EPS preview), EPS/PS (embedded TIFF/WMF/EPSI or Ghostscript),
  WMF/EMF (GDI), and CorelDRAW CDR (embedded preview). HEIC/AVIF and JPEG
  2000 use Windows Imaging codecs when installed. Camera RAW (CR2/CR3, NEF,
  ARW, RAF, ORF, PEF, RW2, SRW, X3F, RWL, DNG, DCR) demosaics via `rawler`
  when the camera is known, otherwise shows the embedded JPEG preview.
  Shift+L `develop` / `exposure=` / `temp=` / `tint=` re-develops from the
  sensor (camera WB + EV). Animated GIF, APNG, and WebP play in the viewer
  (play/pause, frame step, frame strip); export writes sibling
  `stem-f001.png` files and never overwrites the original. Multi-page
  TIFF and multi-size ICO/CUR step like still pages (Shift+←/→, strip);
  Extract page writes `stem-p002.png` or `stem-32x32.png`. Multi-page
  PDF stays in the PDF widget; Extract page writes `stem-p007.png`.
- Image share / export: save-as with JPEG quality and PNG compression,
  optional max-edge resize, ICO and favicon, pixel copy / paste, set as
  wallpaper, email attachment (auto-resized JPEG + `.eml`), social share
  (clipboard + compose URL), and screenshot (screen / window / region,
  optional delay). Viewer panel (`Shift+S` / ↗, `Ctrl+C` / `Ctrl+V`);
  FM **Tools → Image edit**.
- FM **F3** opens the Lister (view), **F4** opens the built-in editor;
  context menu **File associations…** opens the OS default-apps settings.
- Text Lister: Text / HEX / binary, encoding picker, wrap/no-wrap, find
  (F7) with regex + multiline replace, print, undo/redo.
- Media viewer plays in-app via libmpv when bundled; otherwise opens a Play
  handoff to the system player. HTML uses an embedded WebView2 preview
  when the runtime is present, with source toggle and Open in browser.
- **Tier-1 DOCX document editor**: OOXML read/write, Preview/Source,
  parley+swash canvas, selection and keyboard editing, tables (cell nav,
  insert/delete row/col, merge/unmerge cells, `tblGrid` widths, `gridSpan`/`vMerge` preview),
  inline images (body + cells),
  Find/Replace (`Ctrl+F` / F3, `n/m` status, Preview scroll-to-match, Find/Link hover tips, format/align/list (`numbering.xml` preserved when numIds resolve; Orchid `ilvl` 0..=8) + image/table + font/Preview + Save/Print + spacing/indent/page toolbar tips), Preview zoom (Z± / Ctrl+wheel / Ctrl+0, 50–300%), HiDPI 2× preview raster, paragraph/cell shading (`w:shd` / `w:tcPr/w:shd` + preview; Shd toggle), table cell borders (`w:tcBorders` + preview; Bdr toggles cell box when caret in table), all capitals (`w:caps`; Aa toggle), small capitals (`w:smallCaps`; Sc toggle), hidden text (`w:vanish`; Hd toggle), character shadow (`w:shadow`; Sw toggle), emboss (`w:emboss`; Em toggle), imprint (`w:imprint`; Imp toggle), double strikethrough (`w:dstrike`; S2 toggle), heading outline level (`w:outlineLvl`; Ol/P/H1–H9 cycle), named paragraph styles from `styles.xml` (`w:pStyle` round-trip; Heading1–Heading9 applied with Ol cycle; Preview merges style run props + style `w:pPr` spacing/indent/align/shade/borders), named character styles (`w:rStyle` + `styles.xml` type=character round-trip; Preview merges under direct formatting; Ch cycles style on selection), DOCX comments (`word/comments.xml` + `w:commentRangeStart`/`End` round-trip; Cm inserts / Cx deletes / Ce edits text / Cn·Cp jumps next·prev at caret; Preview amber wash on ranges; status shows comment count + caret comment preview), insert/edit/remove hyperlinks
  (Link toolbar / Ctrl+K; internal #bookmark; Bm inserts bookmark at caret), print (toolbar / Ctrl+P), page breaks (Ctrl+Enter), keep-with-next (`w:keepNext`; Kn toggle), keep lines together (`w:keepLines`; Kl toggle), widow/orphan control (`w:widowControl`; Wc toggle), contextual spacing (`w:contextualSpacing`; Cs toggle), RTL paragraph (`w:bidi`; Bi toggle; Preview mirrors left/right align), suppress auto hyphenation (`w:suppressAutoHyphens`; Hy toggle), paragraph spacing (Sb±/Sp±), line spacing (auto Ln± + exact/atLeast round-trip), paragraph indent (w:ind left/right; Ind± / Ir± / Fl±), paragraph box border (`w:pBdr` all sides; Bdr toggle), IME composition (preview), page margins from `pgMar` in preview (Mar+/-), Letter/A4 page size (Pg; keeps orientation) and landscape toggle (Or; `w:pgSz`/`w:orient` round-trip; Pg/Or/Mar/H±/F±/Tp/Ee edit the caret section `sectPr`, not only trailing `page_setup`; Sec inserts next-page section break / cycles continuous on existing end (`w:type`; Preview skips page band for continuous); Sec stamps caret-section geometry), default header/footer relationship + story round-trip (`w:headerReference`/`w:footerReference`, `word/header*.xml`/`footer*.xml`; Preview paints margin stories; save allocates missing parts; Hdr/Ftr toolbar edits plain-text stories (preserves PAGE/DATE/… fields + run/para props); Hf/Ff edit first-page stories; He/Fe edit even-page stories; Preview paints first/even/default header/footer per page-break band; `w:pgMar` `@w:header`/`@w:footer` distances round-trip and Preview honors them; `PAGE`/`NUMPAGES` fields in stories (`w:fldSimple` + complex `w:fldChar` collapse; Preview substitutes per page-break band; Pg# inserts into footer; H±/F± nudge `w:pgMar` header/footer distances; mid-body section breaks via `w:pPr/w:sectPr` round-trip + Sec toolbar / Preview page band; Preview uses per-section header/footer distances + titlePg/evenOdd + body margin x0/wrap + per-section pgSz content width; `DATE`/`FILENAME` fields (`w:fldSimple` + complex collapse; Date/Name toolbar; Preview resolves)); first-page `w:titlePg` + `w:type="first"` header/footer stories round-trip; Tp toolbar toggles `w:titlePg`; even/odd `w:evenAndOddHeaders` + `w:type="even"` header/footer stories round-trip; Ee toolbar toggles `w:evenAndOddHeaders`), status word/char counts, find match-case (Aa), catalog **Document** launcher,
  canvas dock; Word/LibreOffice fixtures and round-trip tests.
- Spec + Phase 1–5 for native **`.orchid`** container (`crates/orchid-format`:
  ORCD/ORCR framing, FlatBuffers TOC, zstd Clean-Text/Structured, `memmap2`
  open, per-region age encryption, linked `ChunkStore` mode with sealed↔linked
  repack, Version-history stub, C2PA Provenance via signed PNG carrier, in-crate
  RGA text CRDT Structured (`orchid.structured.crdt.v1`) with deterministic
  concurrent merge, hierarchical Embedding region (`OREM` /
  `orchid.embedding.hier.f32.v1`), CLI create/read with `--passphrase` /
  `--sign-c2pa` / `--verify-c2pa`) — see [`docs/ORCHID_FORMAT.md`](docs/ORCHID_FORMAT.md).
- Phase 5 hybrid search: `crates/orchid-embed` (`StubEmbedder` for CI; ORT
  feature reserved), `orchid-search` ANN + RRF fusion with Tantivy BM25 and
  `.orchid` Clean-Text extractor (semantic hit when BM25 misses). Live indexer
  enables `with_orchid()`; FM **Wrap as .orchid** packs selection into sealed
  containers (`wrap_as_sealed`).
- Document editor native `.orchid` save/open: sealed envelope with Raw=DOCX +
  Clean-Text=`plain_text`; catalog **Document** creates `Untitled.orchid`;
  with a `ChunkStore` that Untitled file is **linked** from the start.
  `.docx` export path unchanged when the open path ends in `.docx`.
  **Save As…** (toolbar / Ctrl+Shift+S) picks `.orchid` or `.docx`.
  With the app `ChunkStore`, editor saves prefer **linked** `.orchid`
  (generation bump + CAS chunks); open resolves linked Raw/Clean-Text.
  Dirty linked documents autosave after a 2s debounce.
- FM **Wrap as .orchid** inside a managed folder writes **linked** envelopes
  when `ChunkStore` is available; elsewhere remains sealed.
  Wrap and document `.orchid` saves attach a StubEmbedder Embedding region
  when Clean-Text is non-empty (linked writer supports `CAP_EMBEDDINGS`).
- Desktop install script associates `.orchid` /
  `application/vnd.orchid` with `orchid.exe` (per-user HKCU).
  `orchid.exe` opens argv / Explorer-associated paths in a viewer after
  the main window shows. A second `orchid.exe` forwards those paths to the
  already-running instance over a named pipe (Windows single-instance) and
  exits.
- Document **Save As…** from `.docx` → sealed `.orchid` names Raw
  `original.docx` (fidelity import); later native saves use `document.docx`.
- Linked `.orchid` writes preserve Raw TOC `name` / `content_type` (FM wrap +
  document save + sealed↔linked repack).
- Opening a `.orchid` wrap dispatches by Raw MIME/name: PDF/image/text/media/…
  viewers unwrap Raw to a temp file; DOCX envelopes stay in the document editor.
  The viewer chrome keeps the `.orchid` path; unwrap temps are deleted on close.
- Document info strip shows `.orchid` TOC **generation**, linked vs sealed, and
  C2PA verify status when Provenance is present.
- Sealed document saves and FM sealed wraps sign a C2PA Provenance carrier over
  Clean-Text by default (ephemeral signer).
- Encrypted `.orchid` documents prompt for a passphrase on open; the unlock
  identity is retained so Save / autosave re-encrypts instead of stripping
  protection.

#### Built-in widgets
- **System graphs:** CPU, memory, disk, network, and battery rows draw the
  last 60 samples. Network is scaled to the peak in that window. The samples
  stay in memory for the widget instance.
- **Audio Player**: local music library (Songs / Artists / Albums / Folders / Genres),
  playlists (create / rename / delete / add tracks) and favorites, shuffle /
  repeat, sleep timer, EQ presets, ReplayGain, playback speed presets, soft
  soft volume boost (to 150%), library search, add-to-queue / play-next / reorder /
  clear queue, soft **crossfade** (Off / 3 / 5 / 8 / 12s via button or `X`), gapless prefetch,
  sidecar `.lrc` lyrics plus expandable scrollable lyrics panel (`L` / chip; auto-scroll to the
  active line; click a synced line to seek; panel open state + height persist) and ID3 `SYLT`/`USLT`
  plus Vorbis/FLAC comment lyrics when no sidecar, background library scan,
  libmpv duration probe for tracks without ID3 `TLEN` (library rows + queue remaining),
  focused keyboard transport, Windows SMTC (lock screen / media keys), shared
  audio-only libmpv session (separate from SMTC Now Playing and Viewer media
  chrome); widget-owned volume (not Viewer `media_prefs`); mutual pause with
  Viewer. Library list scrolls in the middle so the now-playing bar stays inside
  the widget when resized. Play / enqueue an entire artist, album, or folder
  group; library root chips with per-folder remove; Explorer drop of folders
  (add roots) or audio files (enqueue); play / enqueue all tracks matching the
  active library search (including filtered queue); queue tab play-at without
  reshuffling shuffle order, Home/End jump, **Jump to current** (`.` key),
  **Reshuffle** remaining (keeps current first; `H` on Queue), Queue list follows
  shuffle play order (reorder / Home / End / M3U / save-as-playlist use that order),
  auto-scrolls to the current track when it changes or the Queue tab opens,
  queue strip shows remaining tracks/time from the current row,
  Delete removes current track, empty
  queue hint; Enter plays all search matches; library sort (Artist/Title/Album/Year/Genre);
  favorite toggle on now-playing (star + F key); double-click a library track to
  enqueue; localized Favorites playlist name and library stats strip; file manager
  **Play in Audio Player** / **Add to queue** for audio selections; **Export** /
  **Import M3U** on queue and playlist tabs (import appends to the queue or creates
  a playlist); Escape clears rename / search / browse drill-down; **Folder** button
  reveals the current track directory in the file manager; **G** reveals folder,
  **C** copies the track path. Restores the persisted queue track
  into mpv paused on startup; browse Back shows a human breadcrumb; now-playing
  shows album; playlist rename prefills the current name; EQ / ReplayGain / speed
  persist in widget config and survive track changes. **Recent** playlist
  (last 50 played tracks, chronological). Library / queue rows show ID3
  duration when TLEN is present. Digits **1–7** switch browse tabs
  when the player has focus Library/queue rows show cover thumbnails (APIC / folder cover).
- **Video Player**: dockable local video library (folder drill-down + play/enqueue
  group, queue with play-next / reorder / reshuffle / jump-to-current /
  remaining strip, shuffle/repeat, libmpv RGBA surface, Windows SMTC) — catalog
  creates the widget (no longer a one-shot file-picker → viewer launcher). File
  Manager **Play in Video Player** / **Add to queue**; Explorer drop onto the
  player.
- Terminal (PTY: PowerShell / cmd / WSL / SSH; tabs + splits).
- Weather, Moon (geometric phase disk), System indicators, Media, RSS,
  Recent files, Universal search, Password manager (KDBX4 + Windows Hello).
- **Calculator** (standard / scientific, history, memory, `=expr` search).
- **Processes** (apps / services / startup / users).
- **World clock** (multi-city, IANA zones, GPS/IP “Local” label).
- **Notes** (tabbed scratchpad, wrap/mono/font, find).
- **Calendar** (month grid, day agenda, upcoming strip, jump-to-date, color
  filter, duplicate, year jump, universal search).
- **Jyotish** (Vedic panchanga Phases A–H): day scores, dashas, gochara,
  birth-time rectification, multi-location + GPS/IP pin, birth profiles,
  notifications / export / search, full i18n chrome — see
  [`docs/jyotish.md`](docs/jyotish.md).

#### Platform
- Event bus, action dispatcher, command registry, gesture recognizer,
  shortcut overrides, `BackgroundJobQueue` for always-on fetch work.
- redb state store + TOML config with hot-reload; history / cache eviction.

#### Maturity (0.1)
- Password widget: **edit** existing entries, **group** chips + picker /
  new group, dedicated **generate** sheet (copy without saving).
- In-app **backup** (`orc data export backup`) and **support bundle**
  (`orc diagnostics export`) — zip via a save dialog.
- Settings: first day of week is a combo and drives the calendar. Dead
  haptics / palm / pen rows are hidden (keys stay in `config.toml`).
- Universal-search **files** use ANN + BM25 reciprocal rank fusion
  (`SearchEngine::search_hybrid`, stub embeddings). Semantic hits work
  after the indexer extracts text; `.orchid` Embedding regions are reused
  when present. The ANN is snapshotted to `ann.stub.v1`.
- PDF: `Ctrl+A` selects the current page’s extracted text (copy / highlight
  then use the selection). **Highlight** writes into the open file (sibling
  `*-hl.pdf` if the path is not writable). **Comment** pins a sticky note
  with the selected text (`*-note.pdf` fallback).
- Cinema kit: focus rings, Space/Enter activation, and `accessible-*`
  names on buttons / chips / switches. Floating-window chrome tooltips
  (dock, undock, min, max, restore) were missing.
- Property-style tests: `.orchid` framing / embeddings / zstd / CRDT wire,
  `orc` command tokenizer, OOXML core-props XML and pack line.
- Opt-in **Windows notifications** (`[general].os-notifications`): each
  in-app alert also goes to Action Center. Off by default; unpackaged
  builds write Start Menu `Orchid.lnk` with AppUserModelID `IonPmp.Orchid`.

### Documentation
- The libmpv playback session documents its public frame, transport, and
  command fields.
- Guides, crate READMEs, and the backup manifest now match this tree:
  mail, contacts, CalDAV, the agent, ONNX search, and the files the
  backup zip leaves out (`data/mail`, the agent transcript, face data,
  telemetry).
- Rebuilt project, GitHub, [user](docs/user/README.md), and
  [admin](docs/admin/README.md) docs from the current tree (13 crates,
  `.orchid` Phases 1–5, WebView2 Browser, cinema kit). Planned work is
  listed only in [`docs/ROADMAP.md`](docs/ROADMAP.md).
- Command inventory: [`docs/commands.md`](docs/commands.md) (`orc` verbs vs
  `fs.*` actions). Lua scripting is not planned.
- Password, backup, and support-bundle how-to in the user/admin guides.

### Changed
- **Jyotish**: the day view puts the week strip under the date. Birth date,
  time, and place open one at a time, and birth time uses steppers instead of
  minute wheels. Rectification sits above the life-year list; event years
  step by one or by ten.
- **Spreadsheets**: the open cell table is shared across snapshot ticks, so
  panning the viewer does not copy every cell string first.
- **Terminal tabs**: a snapshot fills pane and divider geometry for the
  active tab. Other tabs still carry their title and focus.
- **File manager**: an ASCII quick filter or Find name matches in place.
  Sorting by size or date no longer lowercases every file name.
- **File manager chrome**: tabs show the folder name and scroll instead of
  the full path overflowing the pane. The path is the address bar on the
  same row as back, up, and the view and sort menus. The places list stays
  visible in a single pane. The drive button shows the current root. Details
  columns shrink to the pane, and the header stays visible while scrolling.
  A narrow pane keeps back, forward, up, the address, view, and sort; home,
  history, a new folder, and branch return as the pane widens. The quick
  filter is a button until the field fits.
- **Terminal resize**: a content tick reuses the frame size already read
  while patching the row, instead of cloning that row again to measure the PTY.
- **Terminal output**: the first chunk records which widget owns the
  session, so later PTY data does not walk every layout.
- **Workspace frames**: a content tick patches the existing row and skips
  the layout snapshot while nothing is being dragged or resized.
- **Alacritty grid**: after the first frame, only damaged lines are copied
  into the snapshot. A scroll still redraws the viewport.
- **Linked documents**: chunk reads run eight at a time and the output buffer
  is sized from the table of contents, instead of waiting on one chunk file
  after another.
- **Archives**: listing and stat reuse a parsed table of contents while the
  archive file is unchanged, instead of re-reading the central directory on
  every folder step.
- **Images**: metadata hashes stream the file and tag parsers keep the first
  4 MiB. Embedded JPEG previews are hunted in the header instead of the
  whole mmap. Loading mpv goes through the unsafe libloading entry point.
- **Search**: PDF indexing binds Pdfium once per worker thread. ANN top-k
  clones only the winning paths. Document count no longer reloads segment
  metadata, and the search widget maps candidates once.
- **Terminal**: a grid row that nothing else shares is edited in place, so a
  paste no longer copies the whole line per character. Unchanged frames share
  the cell buffer instead of cloning every cell on the snapshot tick.
- **File manager**: each listing patch builds visit history once. Entry text
  looks up the path cache without allocating a key on a hit. An empty quick
  filter slices the visible window instead of a pointer per directory entry.
  Arrow keys, shift-range, and the status-bar size walk the live listing
  (and stop once every selected file is counted) instead of copying every
  path or scanning a directory with nothing selected.
- **Recent files**: list rows use the shared **ListTile** control instead of
  custom row chrome (`c0f35fae`).
- **RSS**: feed item rows use **ListTile** (`c0f35fae`).
- **Jyotish**: profile rows use **ListTile** (`efd6b45c`).
- **Clock**: city rows use **ListTile** (`cebc2060`).
- **Search**: candidate rows use **ListTile** (`b149b73a`).
- **Processes**: chrome densified to Theme tokens without flattening the
  table (not ListTile) (`1565dadb`).
- **Calendar**: densified chrome to Theme tokens (`1f11a32a`).
- **Weather**: remaining chrome densified to Theme tokens (`c834c9a4`).
- **Performance**: terminal full redraws paint once instead of twice, with
  cheaper glyph blits (`f158f99f`); frame patches find their row via an index
  hint instead of cloning every wide frame row (`c8c554b1`); audio track
  covers upload only for changed rows (`d40a4cc6`); unchanged System and
  Processes rows are no longer rewritten each sample (`478f1f4d`); the image
  viewer's animation frame strip is no longer remounted on every pan, zoom
  or GIF frame (`68550dd7`); audio and video player libraries and queues
  (`8142edc3`) and the Processes Services, Startup and Users tabs
  (`0c3bd7d9`) build only the rows on screen.
- Workspace crates: `rawler` 0.8, `rfd` 0.17, `libloading` 0.9,
  `zstd` 0.14, plus a semver-compatible `Cargo.lock` refresh.
  Pin `keepass` to 0.13.22 (0.13.25's `aes 0.9` conflicts
  with `age`'s `aes-gcm ^0.10`/`aes 0.8` until `age`
  adopts `aes-gcm 0.11`). Adapt the Slint UI to Slint
  1.17: `Image.source-clip-*` are now `int` (wrap in `round(… / 1px)`),
  repeater `z` must be a number literal (drop the live-frame
  ternary until Slint 1.18 re-allows dynamic `z`), and
  `uuid::encode_lower` now returns `&mut str` (reborrow via `&*`
  instead of the unstable `str_as_str` `.as_str()`).
- **File-manager rendering throughput**: cache per-entry formatted text
  (size / date / type / display name) so an unchanged listing no longer
  re-runs ~288 Fluent `tr_args` lookups plus `chrono` format parses per
  snapshot; cache `format_byte_size` in the `LocaleManager`; and skip
  rebuilding the ~96 `FmEntry` rows on scroll / selection / transfer ticks
  where the visible window did not move, syncing only thumbnail
  appearances in place.
- **Denser widget chrome**: shrink frame headers and in-widget toolbars to
  ~42px (`Theme.header-height` / `Theme.toolbar-height`), use compact
  `IconButton` (`ControlSize.sm`) in widget frames, and size FM toolbar
  shells to `icon-button-sm` so the filter field keeps a usable min-width
  (`1d0ca83a`). Tab strips (group, FM, terminal, notes) match the same
  density with `Theme.toolbar-height` / `Theme.control-sm` while keeping
  finger-sized close / action targets (`490959cb`).
- **File-manager breadcrumbs**: densified to match compact chrome
  (`cf1b9415`).
- **Browser find bar**: densified to match compact chrome (`d49a4f69`).
- **Audio/video player chrome**: densified to Theme tokens (`b32475a5`).
- **LongPressArea** `forward-pointer-events` opt-in forwards inner
  `pointer-event` / `moved` when nested controls still need clicks
  (`5797fb9c`).
- **File manager**: long-press opens the context menu via **LongPressArea**
  (`4450f8f4`).
- **Workspace orb**: swipe left/right to cycle workspaces; tap still
  toggles the menu (`cc0ca3fd`).
- **Touch-first "cinema" redesign of the whole interface.** `Theme` in
  `theme_global.slint` now derives a full design system from the seven raw
  theme colours: surface ramp (sunken → glass → floating), hover / press /
  selected state layers, accent and feedback ramps, radius / spacing /
  elevation / motion scales, cinematic gradient brushes, and density-aware
  `control-*` metrics so every tap target stays finger-sized under any theme.
  A new control kit (`kit.slint`, `kit-icons.slint`) provides `TouchButton`,
  `IconButton`, `Chip`, `ToggleSwitch`, `TouchSlider`, `ProgressTrack`,
  `GlassCard`, `BottomSheet`, `LongPressArea`, `ListTile`, `Badge`, `Tooltip`,
  `EmptyState`, and a stroked
  vector icon set that replaces the ASCII and emoji glyph buttons. Shell
  (welcome screen, widget frames, group tabs, workspace orb, dock), overlays
  (command palette, settings, notification centre, widget catalog, onboarding),
  and widgets (calculator, clock, weather, media, moon, system, RSS, recent
  files, notes, calendar, browser, processes, file-manager toolbar / tabs /
  sidebar / breadcrumbs, terminal tabs, audio and video players, viewer states,
  confirm dialogs) were rebuilt on those tokens. The players get a hero play
  button with an accent bloom and seek rails that thicken under the finger;
  calendar shares a single event-colour palette; the browser gains a tabbed
  strip with an accent underline and a focus-lit address pill.
- Overlays animate in: dialogs, palette, settings, catalog, and the tour fade
  and rise into place, and the notification centre slides in from its docked
  edge. All of it collapses to instant when "reduce motion" is on.
- Touch gestures: notifications can be swiped away in either direction (the
  card tracks the finger and fades as it goes), and swiping across a widget
  group's tab strip steps through the stack. On narrow canvases the settings
  panel and widget catalog present as bottom sheets with a grab handle instead
  of centred cards. The cinema kit exposes a reusable `BottomSheet` control;
  the file-manager context menu uses it on narrow panes (inline submenu
  expansion) and keeps the cursor-anchored popover on wider layouts.
- Light themes: raised surfaces now brighten instead of darkening, so elevation
  reads correctly on paper backgrounds; Solarized Light, Catppuccin Latte, and
  High Contrast Light had their canvas / panel tones swapped to match. Text on
  a solid accent fill is chosen from the accent's own brightness rather than
  from the theme being dark or light, which fixes white-on-yellow and
  black-on-blue pairings in the high-contrast and Solarized themes.
- Build: `.cargo/config.toml` raises `RUST_MIN_STACK` and `orchid-ui/build.rs`
  compiles Slint on a 256 MiB stack — the generated UI tree overflowed both the
  Slint compiler and rustc on Windows.
- **Compile-time module split**: cut oversized first-party units (document
  editor, file-manager widget, viewer widget, UI `wire_callbacks`) into
  sibling modules so rustc can rebuild them incrementally. Public widget
  and viewer APIs stay the same. Follow-up splits: UI file-manager
  handlers (`fm` drag / nav / dialogs / outcome), OOXML
  `document_xml` parse vs write, and document layout (flow / tables /
  paint).
- **Document viewer toolbar**: extract shared `DocToolBtn` so Slint/rustc
  spend less stack and memory on the `viewer-document` compile tree.
- **Image viewer chrome**: extract shared `ImageToolBtn` so Slint generates
  fewer duplicate button trees in `viewer-image`; extract and restyle
  sidebar / overlay panels in `viewer-image-panels` on the cinema kit.
- **Archive / HTML viewer chrome**: restyled on the cinema control kit
  (`IconButton` / `DocToolBtn`, Theme control heights, `surface-sunken`
  toolbars).
- **File-manager dialogs**: find, rename, conflict, passphrase, and tag
  dialogs restyled on the cinema control kit.
- **File-manager pane chrome**: restyled on the cinema control kit
  (`fm-pane` status / path / selection chrome on Theme tokens).
- **File-manager list / grid views**: entry rows and tiles restyled with
  cinema tokens (`fm-entry-list`, `fm-entry-grid`).
- **Workspace taskbar**: restyled on the cinema kit (GlassCard strip,
  Chip-like pills) with swipe to step through floating / minimized windows.
- **LongPressArea** kit control for tablet press-and-hold interactions
  (shared by workspace chrome instead of ad-hoc timers).
- **Password widget**: restyled on the cinema control kit to match the
  touch-first shell.
- **Jyotish widget**: restyled on the cinema control kit to match the
  touch-first shell.
- Light-theme cinema contrast: glass / scrim / shadow alphas and ink-based
  edge highlights so panels and GlassCard stay readable on paper
  (`a9df27fd`).
- **PDF / media / text viewer chrome**: restyled on the cinema control kit
  (`IconButton` / `DocToolBtn` / `Tooltip`, Theme control heights, hairline
  separators, `surface-sunken` toolbars).
- Widget Fluent catalogues: leftover English chrome is translated in all 10
  non-English locales (viewer, file manager, audio/video players, processes,
  and remaining widget chrome). Process status / session / startup labels,
  audio unknown artist/album and sleep timer, and empty “Current location”
  fallbacks go through `LocaleManager`. Universal search Jyotish hits
  (titles, subtitles, source badge) and untitled calendar events resolve
  Fluent keys instead of English literals. Western European catalogues
  that were stored as cp1252 mojibake (`é` as `Ã©`, and similar) are
  restored to UTF-8. Remaining English viewer/file-manager parse errors,
  passphrase and annotation hints, and moon libration labels are
  translated (command-syntax crumbs stay in English). Window-manager
  cap and dock-failed notifications are translated in all locales. The
  default Jyotish birth-profile name (`Profile`) is resolved through Fluent.
- **Video Player** library/queue UX brought to Audio Player parity: folder
  drill-down (play/enqueue group), play-next, queue reorder (drag + up/down),
  reshuffle remaining, jump-to-current auto-scroll, remaining-count strip,
  search play/enqueue, Windows SMTC (`MediaPlaybackType::Video`).
- MSRV / pinned toolchain **1.97 → 1.98.0**; Cargo.lock refreshed to latest
  compatible crate versions, plus intentional bumps: `icu` 2.3, `swash` 0.2.10,
  `crc32fast` 1.5, `fontdb` 0.24, `resvg`/`usvg` 0.48, `parley` 0.11,
  `bzip2` 0.6, `base64` 0.23, digest stack (`sha2`/`sha1`/`md-5`/`digest` 0.11),
  and `totp-rs` 6 (Builder API). Binary codec **`bincode` → `bincode_reloaded`
  3** (crates.io `bincode` 3.0 is an unmaintained stub that only emits
  `compile_error!`).
- File-manager selection follows Explorer instead of per-row checkboxes: click
  selects, Ctrl+click toggles, Shift+click extends, and a drag rubber-bands and
  highlights entries live as it crosses them. The toolbar shows **Deselect**
  only when multiple items are selected (empty click / Escape still clears).
- Large dependency refresh (Tantivy 0.26, redb 4, keepass 0.13, age/secrecy,
  notify, portable-pty/vte, viewers stack, ICU, FastCDC, windows/sysinfo).
- Idle CPU, UI lag, FM listing/thumbnail cost, and cold-start work cut
  (virtualized lists, Arc listings, live dir watches, mmap thumbs, coalesced
  weather/RSS fetches, System/Processes live refresh).
- File-manager listing stays interactive while scrolling: rebase the
  virtual window only near the edge, coalesce snapshot patches, skip
  unchanged rows, and hit-test the visible viewport instead of the full
  virtual height (hover no longer waits on a disk-enum / model rebuild).
- File-manager empty-space click no longer sticks in tap-to-toggle: a
  click (including trackpad jitter) clears selection, and marquee starts
  only after a 10px drag measured in one coordinate space.
- File-manager first paint: list names without waiting on managed/encrypted
  catalogs or per-folder marker probes; extract shell icons and image
  thumbs for the visible window first (list mode skips image thumbs);
  hidden tabs skip formatted rows until shown; local folders list in one
  blocking FindFirstFile/readdir pass instead of a per-file async stat;
  folders larger than the virtual window publish the first 80 names before
  the rest of the directory finishes; the listing stays visible while
  loading (status-bar hint instead of a full-pane overlay).
- **UI/render performance pass**: terminal glyph-cache `Arc` sharing, dirty-line
  retained raster, `Arc<[Cell]>` grid rows + mutation-only generation bumps,
  BytesMut PTY reads; in-place Slint model patches for clock / media / password
  / search / recent / calculator (including floating frames); media thumbnails
  pass `Arc<[u8]>` instead of base64; thumbnail service memory LRU, PNG encode
  without RGBA unwrap-clone, and real in-flight coalescing.
- **Runtime / I/O performance pass**: Windows `mimalloc` global allocator;
  Slint built with only `winit-skia` (femtovg / software dropped); default
  log filter `orchid=info`; image decode uses `into_rgba8` instead of
  `to_rgba8`; system widget reuses the battery `Manager`; weather/RSS pause
  on sleep and resume on the remaining interval; RSS sends `If-None-Match` /
  `If-Modified-Since`; weather cities fetch in parallel; action history
  batches up to 32 entries / 2 s; notification-center writes debounce
  750 ms; Tantivy `QueryParser`s are built once per engine.
- **Terminal raster**: ping-pong two pixel buffers so Slint's `Image` does not
  force a full-frame COW detach every tick; payload cells rewrite only dirty
  rows; PTY updates patch pane pixels in place instead of rebuilding the
  workspace frame; root `terminal-pixels` is no longer rasterized twice.
- **rclone I/O**: `read_stream` pipes `cat` stdout instead of buffering the
  whole file; `metadata` / `exists` use `lsjson --stat` instead of listing
  the parent directory; copy progress is throttled to 150 ms.
- **Widget layout persist**: move / resize / placement reuse the last
  `save_state` blob and flush in one redb transaction after 200 ms;
  canvas group resize uses a single `move_and_resize`.
- **JPEG thumbnails**: decode via libjpeg-turbo IDCT scale (1/2–1/8)
  then a cheap Triangle resize instead of a full Lanczos3 decode.
- **PDF page cache**: worker keeps up to 8 rasterized pages (32 MiB)
  so page / zoom toggles skip Pdfium when the viewport matches.
- **Zero-copy / serialization pass** across the storage and network chains:
  - `ChunkStore::get` no longer opens a redb **write** transaction per chunk
    just to stamp `last_accessed_at`; touches are coalesced in memory and
    flushed in one transaction (every 256 reads and on drop). Reassembling a
    linked `.orchid` cost one fsync per chunk — roughly one per megabyte of
    document — on a pure read path.
  - `ChunkStore::put_with_hash` re-checks the row inside the insert
    transaction and bumps instead of overwriting, so a chunk registered
    concurrently no longer has its refcount reset to 1.
  - rclone RC listings are deserialized **once**, straight from the socket
    buffer into typed rows. They previously went through a `serde_json::Value`
    DOM, a deep clone of the `list` sub-tree, a re-serialize to `Vec<u8>`, and
    a second parse — five materializations per directory listing. Only the
    HTTP status line is decoded as text now; the body stays borrowed bytes.
  - `rclone cat` streams are bound to the reader's lifetime (`kill_on_drop`),
    so sniffing 4 KiB of magic bytes on a remote file no longer leaves rclone
    downloading the entire object.
  - rclone listing rows move their `Name` and `MimeType` out of the parsed
    struct instead of cloning both per entry.
  - Tantivy's `reader.reload()` moved off the per-query path onto `commit`,
    which is already coalesced at 750 ms — search stopped re-reading segment
    metadata on every keystroke.
  - One process-wide pooled `reqwest::Client` (`orchid_ui::http::shared_client`);
    weather's "use my location" was building a fresh client, TLS root store and
    connection pool per click.
  - IP geolocation parses response bytes directly instead of round-tripping
    through a `String`; the RSS poller clones cached items only on a 304
    instead of deep-cloning every `FeedItem` on every fetch.
- **i18n lookups are memoized**: argument-free `LocaleManager::tr` caches
  resolved strings (new `tr_shared` returns the shared `Arc<str>`), cleared on
  locale switch. Building one widget frame resolves ~650 Fluent patterns, so
  this lands on every workspace rebuild — resize, workspace switch, and the
  flush at the end of every drag / resize gesture.
- **UI frame remount pass**: viewer / terminal / processes / common-content
  patches skip `set_row_data` when only nested ModelRc content changed
  (Arc-shared with the live row); group-tab rebuilds are gated the same way;
  ungrouped widgets share one empty group-tab ModelRc per UI thread; PDF
  outline/overlay ModelRc identities are preserved across page changes;
  frame-row lookup encodes the UUID on the stack instead of `to_string()`.
- **Search remove coalescing**: directory-tree deletes no longer force a
  Tantivy commit per file; they ride the existing 750 ms commit window.
- **HTML webview**: skip host-thread wake when the document body is unchanged
  across viewer content ticks.
- **Tag multi-select**: star / unstar / colour / add-tag / remove-tag across a
  selection commit in one redb transaction instead of one fsync per path.
- **rclone RC keep-alive**: one persistent TCP connection to `rclone rcd` with
  HTTP/1.1 keep-alive and `Content-Length` framed reads, so folder navigation
  does not handshake `127.0.0.1` on every list/stat.
- **FM metadata hydrate**: skip re-stat when listing already filled
  kind/size/mtime/mime (directories always skipped); only bare catalog paths
  still pay a metadata round trip.
- **Empty widget-frame pack**: cache the ~19 empty sibling Slint models per
  locale on the UI thread so `build_widget_frame_for_placed` clones refcounts
  instead of re-resolving ~650 Fluent labels per frame.
- **Managed-folder config cache**: `list_folders` results stay in memory and
  are invalidated on add/remove, so the auto-ingest loop no longer re-decodes
  every folder config up to three times per filesystem event.
- **Processes widget**: full process census on activate and every 4th
  tick; intervening samples refresh only last-known PIDs.
- **DOCX preview**: caret / selection paints over a cached page raster
  instead of relayouting every paragraph; `LayoutCache` is used on the
  render path.
- **rclone list/stat**: one localhost `rclone rcd` serves `operations/list`
  and `operations/stat` over RC HTTP; CLI is the fallback if the daemon
  is missing.
- **Managed-folder ingest**: FastCDC and the whole-file BLAKE3 share one
  read; files over 16 MiB hash with `update_mmap_rayon`.
- **Thumbnail disk cache**: stores packed RGBA instead of PNG so a hit
  does not decode an image.
- **PDF render queue**: worker channel is bounded; queued page / zoom
  flips for the same session collapse to the latest request, and the
  viewer ignores superseded rasters.
- **Text viewer open**: local files stay memory-mapped; the rope is
  filled via `from_reader` so UTF-8 is not copied into an extra
  `String`. Encoding switches re-decode the mapped bytes in place.
- **Video player**: publish on a new frame or a one-second progress
  tick instead of every 100 ms while playing; Slint reuses the last
  frame `Image` when the RGBA `Arc` is unchanged.
- **Snapshot cache**: event-driven refreshes skip the frame when
  render-equality matches (System / Processes compare live values).
- **Image slideshow**: sleep until the next transition tick or slide
  change instead of waking every 50 ms.
- **Image checkerboard**: cache the composited RGBA by source `Arc`
  so a viewer rebuild does not copy a full-resolution frame again.
- **UI tick**: 33 ms (~30 Hz) instead of 16 ms; the first paint shows
  the shell and rebuilds workspace frames on the first tick.
- **Viewer patch**: text lines and image strips keep their nested
  `VecModel`s; only changed rows are rewritten.
- **Weather / Notes / Calendar**: content ticks patch nested `VecModel`s
  instead of remounting the widget frame.
- **Content refresh**: clock / weather / notes / calendar / processes /
  jyotish / FM navigation / password / widget settings patch one frame
  instead of rebuilding the workspace grid.
- **Command palette**: arrow selection updates `selected_index` without
  rebuilding the candidate list.
- **Jyotish / Moon / RSS / audio / video**: snapshot ticks patch nested
  lists and reuse cover / frame `Image`s instead of remounting the widget.
- **Viewer archive / document / media**: keep listing and playlist
  `VecModel`s across extract, caret, and playback ticks.
- **Event bus**: OSC 52 clipboard writes leave the sync dispatcher.
- **Commands**: shortcuts and the command palette patch dirty widget frames
  instead of rebuilding the workspace grid; structural commands still rebuild.
- **Command palette**: query keystrokes debounce 50 ms and skip candidate
  rows that already compare equal.
- **Local copy**: same-volume files use `CopyFileExW` (Windows) or
  `std::fs::copy` instead of a 128 KiB user-space stream.
- **Event bus dispatch**: typed subscribers are indexed by event type so
  each publish skips unrelated filters.
- **Dist profile**: `cargo build --profile dist` inherits release and sets
  `panic = "abort"` for packaged / PGO builds.
- **Widget lookup**: `instances_for_workspace` uses a workspace id index
  instead of scanning every live instance.
- **Tokio**: workspace features are `rt-multi-thread` / `macros` / `sync` /
  `time` / `fs` / `io-util` / `parking_lot`; `net` and `process` stay on
  orchid-fs only. `signal` and `io-std` are off.
- **image**: default decoders drop `avif` (WIC), `qoi`, and `farbfeld`.
- **rclone copy progress**: stdout is discarded (`Stdio::null`) so a full
  pipe cannot stall the child while only stderr is read.
- **Widget groups**: workspace lists and `find_for_instance` use id indexes
  instead of scanning every tab stack.
- **Audio / video libraries**: `find_by_path` is a HashMap lookup after scan.
- **Canvas pan**: wheel / trackpad scroll updates the Flickable offset
  without waking or sleeping widgets on every tick; visibility sync
  waits until motion stops, skips an unchanged id set, and does not
  `touch()` widgets that are already Active.

### Fixed

- Audio and video browse tabs, and audio playlist chips, stay as wide as
  their labels instead of collapsing in the strip.
- **Jyotish**: locating and failed states stay on Current location. The
  place row shows the UTC offset beside the place name. Profile edit and
  remove buttons have tooltips, and the editor closes when editing ends.
  An event year stays inside the birth-to-next-year span. Month days, year
  rows, life years, and birth-date cells activate from the keyboard.
- **Contacts:** the desktop app registers the contacts widget, so Add from
  the catalog creates it.
- Audio and video players keep transport controls on screen: seek and volume
  are finger-sized sliders, library actions scroll instead of being clipped,
  and track buttons use icons with readable labels. Lyrics follow the
  current line. The video player accepts a volume drag.
- The mail widget is translated in every bundled locale.
- Password editing, backup and diagnostics export, and dock labels that
  were still English are translated in every bundled locale.
- Remaining interface labels (playlists, shell, wind speed, byte sizes,
  and editor hints) are translated where the English word is not the
  local term. Sanskrit glossary terms stay as transliteration.
- Full-text search indexes `.proto`, `.graphql`, `.gql`, `.prisma`,
  `.nix`, `.tf`, `.hcl`, and `.zig` as source text.
- Full-text search indexes `.aws/credentials`: profile names and regions.
  Access keys and secrets are not indexed.
- Full-text search indexes `.git-credentials`: host names. Usernames and
  passwords are not indexed.
- Full-text search indexes `.pypirc`: server names and repository URLs.
  Usernames and passwords are not indexed.
- Full-text search indexes `.netrc`: machine names. Logins, passwords, and
  accounts are not indexed.
- Full-text search indexes `.npmrc`: registry URLs. Auth tokens and
  passwords are not indexed.
- Full-text search indexes `.env` files: variable names. Values are not
  indexed.
- Full-text search indexes `requirements.txt`: package names. Versions
  and `--hash` values are not indexed.
- Full-text search indexes `MODULE.bazel.lock`: module and repository
  names. Integrity hashes are not indexed.
- Full-text search indexes `Chart.lock`: dependency names and
  repositories. Versions and the digest are not indexed.
- Full-text search indexes `.terraform.lock.hcl`: provider addresses.
  Versions and hashes are not indexed.
- Full-text search indexes `Cartfile.resolved`: repository names and URLs.
  Versions and commits are not indexed.
- Full-text search indexes `Package.resolved`: package identities and
  repository URLs. Revisions and versions are not indexed.
- Full-text search indexes `deno.lock`: package names and remote module
  URLs. Integrity hashes and revisions are not indexed.
- Full-text search indexes `bun.lock`: package names. Versions and
  integrity hashes are not indexed.
- Full-text search indexes `flake.lock`: node names, owners, repos, and
  refs. narHash values and revisions are not indexed.
- Full-text search indexes `mix.lock`: package names. Versions and
  checksums are not indexed.
- Full-text search indexes `pdm.lock`: package names and summaries.
  Versions and file hashes are not indexed.
- Full-text search indexes `Podfile.lock`: pod names. Versions, commits,
  and checksums are not indexed.
- Full-text search indexes `packages.lock.json`: package ids. Versions
  and content hashes are not indexed.
- Full-text search indexes `uv.lock`: package and dependency names.
  Versions and hashes are not indexed.
- Full-text search indexes `pubspec.lock`: package names. Versions and
  sha256 checksums are not indexed.
- Full-text search indexes `Pipfile.lock`: package names. Versions and
  hashes are not indexed.
- Full-text search indexes `Gemfile.lock`: gem names. Versions,
  revisions, and checksums are not indexed.
- Full-text search indexes `poetry.lock`: package names and
  descriptions. Versions and file hashes are not indexed.
- Full-text search indexes `pnpm-lock.yaml`: package names. Versions
  and integrity hashes are not indexed.
- Full-text search indexes `composer.lock`: package names and
  descriptions. Versions and dist checksums are not indexed.
- Full-text search indexes `yarn.lock`: package names. Versions and
  integrity hashes are not indexed.
- Full-text search indexes `package-lock.json`: package names.
  Versions and integrity hashes are not indexed.
- Full-text search indexes `go.sum`: module paths. Versions and
  checksums are not indexed.
- Full-text search indexes `Cargo.lock`: package names. Versions,
  sources, and checksums are not indexed.
- Full-text search indexes `go.mod`: the module path and required
  module paths. Toolchain and dependency versions are not indexed.
- Full-text search indexes `package.json` and `composer.json`: names,
  descriptions, keywords, and dependency names. Scripts and versions are
  not indexed.
- Full-text search indexes Dockerfiles (`Dockerfile`, `.dockerfile`):
  image names, labels, and copy paths. `ENV` and `ARG` values are not
  indexed.
- Full-text search indexes property lists (`.plist`): display names,
  identifiers, and usage descriptions. Version and SDK values are not
  indexed, and binary plists are ignored.
- Full-text search indexes Android manifests (`AndroidManifest.xml`):
  package names, labels, component names, and permissions. Versions and
  resource references are not indexed.
- Full-text search indexes XML solutions (`.slnx`): project paths,
  folder names, and solution-item paths. Build configurations are not
  indexed.
- Full-text search indexes Maven POM files (`pom.xml`): group ids,
  artifact ids, names, and descriptions. Versions and `properties`
  blocks are not indexed.
- Full-text search indexes Visual Studio solutions (`.sln`): project
  names, project paths, and solution items. GUIDs and configuration
  tables are not indexed.
- Full-text search indexes NuGet manifests (`.nuspec`): package ids,
  titles, descriptions, authors, and dependency ids. Versions, commits,
  and packed files are not indexed.
- Full-text search indexes diffs (`.diff`, `.patch`): changed paths and
  line text. Git blob hashes and binary patch bodies are not indexed.
- Full-text search indexes MSBuild projects (`.csproj`, `.fsproj`,
  `.vbproj`, `.vcxproj`): assembly names, descriptions, SDK ids, and
  package or project references. Versions and source-file lists are not
  indexed.
- Full-text search indexes RDoc (`.rdoc`): headings, lists, and
  paragraphs. Rules and `:stopdoc:` blocks are not indexed.
- Full-text search indexes Debian source control (`.dsc`, `.changes`):
  package names, descriptions, and relationships. Checksums and file
  hashes are not indexed.
- Full-text search indexes RPM spec files (`.spec`): package names,
  summaries, descriptions, changelogs, and file lists. Build scripts are
  not indexed.
- Full-text search indexes torrent files (`.torrent`): display names,
  comments, announce URLs, and file paths. Piece hashes are not indexed.
- Full-text search indexes WiX sources (`.wxs`, `.wxl`): product names,
  feature titles, dialog text, and localization strings. Component ids
  and property values are not indexed.
- Full-text search indexes XSPF playlists (`.xspf`): titles, creators,
  albums, and locations. Durations and vendor extensions are not indexed.
- Full-text search indexes systemd units (`.service`, `.socket`, `.mount`,
  and similar): descriptions, documentation, and start commands.
  Environment variables and credentials are not indexed.
- Full-text search indexes GNU Info manuals (`.info`, `.info-1`): node
  titles and body text. Tag tables are not indexed.
- Full-text search indexes compiled gettext catalogs (`.mo`, `.gmo`):
  message ids and translations. The catalog header is not indexed.
- Full-text search indexes Texinfo manuals (`.texi`, `.texinfo`): titles
  and body text. `@ignore` blocks and `@c` comments are not indexed.
- Full-text search indexes Perl POD (`.pod`): headings, items, and
  paragraphs. Code after `=cut` and `=begin comment` blocks are not
  indexed.
- Full-text search indexes SAMI (`.smi`) and TTML (`.ttml`, `.dfxp`)
  cues. Style blocks and timestamps are not indexed.
- Full-text search indexes iOS storyboards and XIB files
  (`.storyboard`, `.xib`): titles, label text, placeholders, and user
  labels. Class names and object ids are not indexed.
- Full-text search indexes manual pages (`.1`, `.3pm`, `.man`, and
  similar): section titles and body text. Comments and `.ig` blocks are
  not indexed.
- Full-text search indexes XAML (`.xaml`) titles, text, content, and
  headers. Element names and `x:Name` ids are not indexed.
- Full-text search indexes Qt Designer and GTK Glade files (`.ui`):
  window titles, labels, and tooltips. Geometry and object ids are not
  indexed.
- Full-text search indexes Windows resource scripts (`.rc`): quoted
  strings from string tables, dialogs, and menus. Comments and
  `#include` lines are not indexed.
- Full-text search indexes Android string resources (`strings.xml` and
  other `<resources>` files): resource names and visible text, including
  text inside `xliff` placeholders. Other `.xml` files stay plain text.
- Full-text search indexes Qt Linguist catalogs stored as `.ts`:
  context names, source strings, and translations. A `.ts` file that is
  TypeScript is still indexed as source. Location filenames are skipped.
- Full-text search indexes Apple string tables (`.strings`,
  `.stringsdict`): keys, translations, and block comments. Line comments
  are not indexed.
- Full-text search indexes .NET `.resx` files and XLIFF (`.xlf`,
  `.xliff`): names, source strings, and translations. Embedded binary
  values are not indexed.
- Full-text search indexes Java property files (`.properties`): keys,
  values, and comments. `\\uXXXX` escapes are decoded, and a trailing
  backslash joins the next line.
- Full-text search indexes registry exports (`.reg`): key paths and
  string values, including UTF-16 files. `dword` and `hex` values are
  not indexed.
- Full-text search indexes visible SVG (`.svg`) labels. Scripts, styles,
  and metadata are not indexed.
- Full-text search indexes media notes (`.nfo`). Kodi XML contributes
  titles, plots, and names; poster URLs and stream details are skipped.
  Plain-text scene notes are indexed as text.
- Full-text search indexes subtitle cues (`.srt`, `.vtt`, `.ass`, `.ssa`,
  `.lrc`) without timestamps, cue numbers, or style overrides.
- Full-text search indexes shortcuts (`.url`, `.desktop`, `.webloc`):
  names and URLs. `Exec` lines and icon paths are not indexed.
- Full-text search indexes Gettext catalogs (`.po`, `.pot`): message
  ids, translations, and translator comments. Source locations (`#:`)
  and flags (`#,`) are not indexed.
- Full-text search indexes CUE sheets (`.cue`): album and track titles,
  performers, songwriters, and file names. `INDEX` timestamps and `TRACK`
  headers are not indexed.
- Full-text search indexes LaTeX sources (`.tex`, `.ltx`). Line
  comments are omitted. A percent sign escaped with a backslash, and
  the body of `verbatim`, `lstlisting`, and `minted`, are kept.
- Full-text search indexes playlists (`.m3u`, `.m3u8`, `.pls`): track
  titles and file or stream paths. Durations and playlist directives
  are not indexed.
- Full-text search indexes RSS and Atom feeds (`.rss`, `.atom`, and
  `.xml` files that open as a feed). Titles, authors, links, and article
  text are kept; markup, scripts, and dates are not.
- Full-text search indexes Unix mailboxes (`.mbox`) as separate
  messages, using the same subject and body rules as `.eml`. A `From `
  line inside a paragraph is not treated as a message boundary.
- Full-text search indexes Jupyter notebooks (`.ipynb`): markdown and
  code cell sources. Cell outputs and embedded images are skipped.
- Full-text search indexes OPML (`.opml`) outline titles and feed URLs
  instead of the raw XML.
- Full-text search indexes BibTeX (`.bib`) and RIS (`.ris`) titles,
  authors, abstracts, and related fields. Citation keys and `@string`
  macros are not indexed.
- Full-text search indexes iCalendar (`.ics`) and vCard (`.vcf`) fields
  (summary, location, name, email, notes). Folded lines are joined.
  `PHOTO` and `ATTACH` values are not indexed.
- Full-text search indexes source and subtitle files (Rust, Python,
  JavaScript, TypeScript, PowerShell, SQL, ASS/SSA, and similar) when
  `[search].extract-text` is enabled.
- Full-text search indexes `.eml` messages: subject, from, to, and
  text or HTML bodies. Quoted-printable, base64, and encoded-words are
  decoded; attachments are skipped.
- Full-text search indexes FictionBook (`.fb2` and `.fb2.zip`): title,
  authors, annotation, and body text. Embedded cover images are skipped,
  and `windows-1251` books are decoded from the XML declaration.
- Full-text search indexes audio tags from MP3 / WAV / AIFF (ID3) and
  FLAC / Ogg / Opus (Vorbis comments), including embedded lyrics.
- Full-text search indexes visible HTML / XHTML text and leaves `.rtf`
  (`text/rtf`) to the RTF extractor instead of storing raw markup.
- Full-text search extracts `.rtf` body text (Unicode and hex escapes;
  font tables, pictures, and `\*` groups are skipped) when
  `[search].extract-pdf` is enabled.
- Full-text search extracts EPUB chapter text in spine order and
  OpenDocument (`.odt` / `.ods` / `.odp`) text from `content.xml` when
  `[search].extract-pdf` is enabled.
- Full-text search extracts Excel cell text (shared strings, inline
  strings, numbers, sheet names) and PowerPoint slide plus speaker-notes
  text when `[search].extract-pdf` is enabled.
- Viewer dispatch sniffs OOXML `[Content_Types].xml` in ZIP heads so Word
  packages open in the document editor while Excel / PowerPoint open as
  archives (including misnamed `.docx` sheets/slides).
- Password manager no longer pretends a secret was copied when the OS
  clipboard is unavailable; copy failures surface a toast instead of a
  silent success.
- Terminal no longer leaves stale rows on screen after a UI stall: the
  retained raster diffs each buffer against the cells it shows instead of
  trusting the emulator's per-snapshot dirty lines (`f158f99f`).
- Closed terminal panes no longer leak their two retained RGBA bitmaps
  (`f158f99f`).
- Playing video no longer fills the viewer image cache with up to 96 stale
  full-size frames (evicting photos and PDF pages); image caches pin their
  source buffer so a reused address cannot show an old frame (`bc83d29f`).
- File manager now reflects pausing a transfer, the transfer queue length,
  selected-size totals, scrolled viewport windows and branch view right
  away instead of waiting for an unrelated change (`4ead829e`).
- Audio and video player queues: jumping to the current track no longer
  slides the list down behind a blank gap, and drag-reordering a scrolled
  queue drops the track where the pointer is (`8142edc3`).
- Image viewer no longer hitches when the widget is dragged or the photo is
  panned: live geometry is applied through AppState overlay properties
  instead of `set_row_data` on the fat `WidgetFrameModel` (which remounted
  the full-resolution frame on every pointer move), the canvas draws only
  the visible `source-clip` of axis-aligned photos, and pan commits to
  Rust on pointer-up.
- File-manager toolbar hover no longer freezes for ~1s: volume enumeration
  (`sysinfo` / WMI) ran on the UI thread during every FM patch, including
  while the pointer moved across Back / Forward / drives. Drive letters
  now come from `GetLogicalDrives` (no per-volume I/O); labels refresh in
  the background. Tooltips wait 400ms and use a lighter shadow so sweeping
  the chrome does not invalidate the whole widget frame.
- Cinema design tokens on light themes: glass / scrim / shadow alphas and
  ink-based edge highlights so panels, modals, and GlassCard edges stay
  readable on paper (orchid-light bases tightened for the same contrast).
- File-manager listings stop scrolling past the last entry: the list and grid
  derive their scroll extent from the rows / tiles they lay out instead of the
  backend's content height, which is computed from the last reported pane width
  and left dead space below the listing after a resize.
- File-manager hover no longer dirties every row's Image/Text tree: a single
  overlay strip tracks the hovered index, decoration snapshot publishes are
  coalesced (prefetch/thumbs), Icons view uses ExtraLarge instead of Jumbo,
  and shell icons are downscaled to display size before caching.
- File-manager exclusive click highlights the new row immediately: Slint paints
  the pressed path before the model patch returns, selection updates stay on
  the UI stack (including floating frames), and patching no longer remounts the
  tab when only `is_selected` flags change — that remount left the previous
  folder lit until Escape forced a full rebuild.
- Image viewer toolbar: compact geometric icons (not font symbols),
  overflow scroll, a hint strip with the action name on hover, extra
  tools behind **⋯**, and slideshow extras only while a slideshow is
  playing. Hover labels no longer fly off-button.
- System CPU sampling via `GetSystemTimes`; process-list refresh spikes.
- FM Type labels for long extensions; delete-to-recycle / show-extensions;
  transfer and rename failure toasts.
- File-manager gallery / large-icon tiles no longer stretch 32×32 shell bitmaps;
  Windows jumbo (256px) association icons are used, and small glyphs stay at
  native size instead of melting across the tile.
- File-manager drag-and-drop starts on the pressed entry (selection lag no
  longer aborts the gesture) and dropping onto a file completes a move/copy
  into the current folder instead of cancelling.
- File-manager listings no longer go blank after leaving a long folder: the
  visible window resets on navigate / tab / filter / sort, and small folders
  always ship a full slice. Scroll virtualization mutates the entry model in
  place instead of remounting the widget (which made selection jump).
- File-manager selection: click empty space clears; marquee only hits tiles
  that intersect the rectangle (empty drag no longer selects the last file);
  checkbox is a dedicated hit target; click-drag of an already-selected set
  keeps the multi-selection; selected rows use a translucent fill so names
  stay readable; Ctrl+click in single-click-open mode no longer opens.
- File-manager selection mode no longer sticks: it turns itself off once the
  selection empties, a press whose release is swallowed by a listing update can
  no longer arm it after 500 ms, and clicks commit against the entry that was
  pressed rather than whatever the recycled row shows at release time.
- File-manager marquee commit keeps the live index range instead of
  re-hit-testing on mouse-up, which often shrank the selection the moment the
  button came up (restored only after a focus-driven full repaint). The model
  is patched before the live highlight is cleared, and selection cache syncs
  no longer mark the frame dirty (that raced the rubber band on the next tick).
- File-manager rubber-band rows highlight immediately from the live index
  range in Slint, and marquee selection updates the model on the UI stack
  instead of an async spawn — the previous round-trip left some entries
  unhighlighted until a focus-driven full repaint.
- File-manager shell icons use `IShellItemImageFactory` (Explorer's path)
  instead of the jumbo image list, which often stores a 32×32 glyph on a
  256×256 canvas and looked melted when stretched. Rows pull 48px sources;
  tiles fill their box from a true 256px bitmap.
- File-manager rubber band tracks the drag again. The band was driven by an
  overlay spawned on press, which never received the drag because the pressed
  area keeps the pointer grab; it only saw the bare cursor after release, so
  dragging selected nothing and merely hovering afterwards selected entries.
  Both views now track the band in the area that holds the grab.
- File-manager dropped the invisible select mode that a rubber band or a 500 ms
  hold used to latch, which silently turned every later plain click into a
  toggle. Toggling is Ctrl+click only, as in Explorer.
- File-manager rubber band can start on an entry, not just on empty space:
  dragging off an unselected row or tile bands, dragging off a selected one
  still transfers the selection. Touchpads had no reachable way to begin a
  selection, since the only entry points were a 500 ms hold and empty space.
- File-manager gestures now handle `PointerEventKind.cancel`. Touchpads abort
  pointer sequences far more often than mice, and an aborted press never
  reached the `up` handlers: the hold timer kept running and armed select mode
  by itself a moment after a light tap, and the latched `press-empty` flag
  froze list scrolling.
- File-manager clicks: Ctrl+click no longer counts as half of a double click,
  a third rapid click no longer opens the file twice, and selection patches in
  single-pane mode stop falling back to a full frame rebuild.
- File-manager background right-click works on empty space and empty folders,
  and shows only relevant actions (new folder / file, paste, select all).
- File-manager context-menu icons render as geometric glyphs (the previous
  `action-*` ids were drawn as Text and do not exist in Slint's Windows font).
- File-manager single-pane mode no longer shows the left navigation sidebar;
  the listing uses the full widget width. Dual-pane still includes the sidebar.
- Floating viewer unsaved-close confirm; Clock move-city handlers; Jyotish
  birth-profile date calendar (month/year sheets) and hour/minute wheels;
  search field sync.
- Fluent message IDs (hyphenated only); locale UTF-8 mojibake from early
  dependency bumps.

### Security
- Prefer `rclone-remote` over plaintext passwords in `config.toml` (documented
  in [`docs/SECURITY.md`](docs/SECURITY.md)).
- Vault idle auto-lock; Windows Hello / DPAPI for vault and encrypted-folder
  passphrase.

---

Older scaffolding history (2026-04 → mid-2026) lives in git; this file tracks
user-visible and contributor-relevant milestones from the pre-alpha push
toward MVP v0.1.
