# Viewers

Opening a file (F3 / double-click / drop) routes by magic bytes and
extension. `.orchid` wraps unwrap Raw (except DOCX envelopes, which stay
in the document editor). Chrome keeps the `.orchid` path; temps are deleted
on close. OOXML packages (ZIP) are classified via `[Content_Types].xml`
when present in the file head: Word → document editor; Excel (`.xlsx`,
`.xlsm`) → a sheet table (switch sheets, click a cell or use the arrow
keys; the bar shows that cell's address and a field for its value; the
first 400 rows and 32 columns). The preview can highlight a cell from at most 8 conditional formatting rules. Each range covers at most 32 cells. A rule can be greater than a plain number, equal to a plain number or a quoted string, or a `containsText` pattern using `*`, `?`, and `~`. A pattern without those characters is an exact ASCII case-insensitive match, not a substring search. A pattern longer than 64 characters, or cell text longer than 256 characters, is not a match. The first matching rule wins. Colors, data bars, color scales, and formula rules are skipped and do not count toward the 8. A range larger than 32 cells is skipped. The highlight is not written back and formulas do not use it. A selected cell can show a legacy comment from the worksheet comments part. At most 32 comments are read, each note is at most 256 characters, and the author is ignored. Threaded comments are not read. The note is not editable and is not written back. The preview can show a short list of number formats without changing the stored number. Percent is built-in format 9 or 10, or a custom format that is exactly `0%` or `0.00%`: the number times 100, trimmed the same way other calculated numbers are trimmed, then `%`. Thousands is built-in format 3 or exactly `#,##0`, and only when the value is a whole number smaller than 1e15 in absolute value. A date is built-in format 14 or exactly `yyyy-mm-dd`, shown as `yyyy-mm-dd` from the 1900 serial, including the fake 29 February 1900. A time fraction is dropped. A 1904 workbook is not shifted. Any other format, and any cell that is not a number, stays the stored text. At most 64 cell formats are read. `TEXT` is still the formula that writes a formatted value. Enter or Save writes that cell back into
the workbook. A formula cell is left unchanged. A value edit recalculates
arithmetic, comparisons, cell references, `SUM`, `AVERAGE`, `MIN`, `MAX`,
`COUNT`, `IF`, `ROUND`, `ABS`, `INT`, text joined with `&` or `CONCAT`,
and `LEN`, `LEFT`, `RIGHT`, `MID`, `UPPER`, `LOWER`, `TRIM`,
`SUBSTITUTE`, `FIND`, `SEARCH`, `REPT`, `EXACT`, `AND`, `OR`, `NOT`,
`SQRT`, `POWER`, `MOD`, `SIGN`, `PRODUCT`, `QUOTIENT`, `PI`, `EVEN`,
`ODD`, `REPLACE`, `VALUE`, `T`, `N`, `ROUNDUP`, `ROUNDDOWN`,
`CEILING.MATH`, `FLOOR.MATH`, `MEDIAN`, `ISNUMBER`, `ISTEXT`, `GCD`, `LCM`, `LN`, `LOG10`, `LOG`, `EXP`, `FACT`, `SIN`, `COS`, `TAN`, `RADIANS`, `DEGREES`, `ASIN`, `ACOS`, `ATAN`, `ATAN2`, `SINH`, `COSH`, `TANH`, `COMBIN`, `PERMUT`, `PERMUTATIONA`, `LARGE`, `SMALL`, `TRUNC`, `ISEVEN`, `ISODD`, `CODE`, `CHAR`, `COUNTA`, `COUNTBLANK`, `IFERROR`, `CLEAN`, `PROPER`, `CHOOSE`, `SWITCH`, `XOR`, `TEXTJOIN`, `IFS`, `BITAND`, `BITOR`, `BITXOR`, `CEILING`, `FLOOR`, `BITLSHIFT`, `BITRSHIFT`, `MROUND`, `SUMIF`, `COUNTIF`, `AVERAGEIF`, `SUMPRODUCT`, `MINIFS`, `MAXIFS`, `SUMIFS`, `AVERAGEIFS`, `SUMSQ`, `STDEV`, `STDEV.S`, `STDEVP`, `STDEV.P`, `VAR`, `VAR.S`, `VARP`, `VAR.P`, `AVEDEV`, `DEVSQ`, `GEOMEAN`, `HARMEAN`, `COUNTIFS`, `SLOPE`, `INTERCEPT`, `CORREL`, `PEARSON`, `RSQ`, `FORECAST`, `FORECAST.LINEAR`, `STEYX`, `COVARIANCE.P`, `COVAR`, `COVARIANCE.S`, `RANK`, `RANK.EQ`, `RANK.AVG`, `PERCENTILE`, `PERCENTILE.INC`, `PERCENTILE.EXC`, `QUARTILE`, `QUARTILE.INC`, `QUARTILE.EXC`, `MODE`, `MODE.SNGL`, `PERCENTRANK`, `PERCENTRANK.INC`, `PERCENTRANK.EXC`, `STANDARDIZE`, `SKEW`, `SKEW.P`, `KURT`, `TRIMMEAN`, `FISHER`, `FISHERINV`, `SQRTPI`, `COMBINA`, `SUMX2MY2`, `SUMX2PY2`, `SUMXMY2`, `GESTEP`, `DELTA`, `MULTINOMIAL`, `FACTDOUBLE`, `POISSON`, `POISSON.DIST`, `BINOM.DIST`, `BINOMDIST`, `EXPON.DIST`, `EXPONDIST`, `NEGBINOM.DIST`, `NEGBINOMDIST`, `HYPGEOM.DIST`, `HYPGEOMDIST`, `WEIBULL.DIST`, `WEIBULL`, `GAMMA`, `GAMMALN`, `GAMMA.DIST`, `GAMMADIST`, `BINOM.INV`, `CRITBINOM`, `CHISQ.DIST`, `CHISQ.DIST.RT`, `CHIDIST`, `NORM.S.DIST`, `NORMSDIST`, `NORM.DIST`, `NORMDIST`, `ERF`, `ERFC`, `GAUSS`, `PHI`, `LOGNORM.DIST`, `LOGNORMDIST`, `BINOM.DIST.RANGE`, `Z.TEST`, `ZTEST`, `PROB`, `ASINH`, `ACOSH`, `ATANH`, `SEC`, `CSC`, `COT`, `CONCATENATE`, `UNICHAR`, `UNICODE`, `SERIESSUM`, `NORM.S.INV`, `NORMSINV`, `NORM.INV`, `NORMINV`, `LOGNORM.INV`, `LOGINV`, `CONFIDENCE`, `CONFIDENCE.NORM`, `GAMMA.INV`, `GAMMAINV`, `CHISQ.INV`, `CHISQ.INV.RT`, `CHIINV`, `ROMAN`, `ARABIC`, `SECH`, `CSCH`, `COTH`, `ACOT`, `ACOTH`, `CHISQ.TEST`, `CHITEST`, `AVERAGEA`, `MINA`, `MAXA`, `STDEVA`, `VARA`, `DATE`, `YEAR`, `MONTH`, `DAY`, `DEC2BIN`, `BIN2DEC`, `DEC2HEX`, `HEX2DEC`, `DEC2OCT`, `OCT2DEC`, `BASE`, `DECIMAL`, `BESSELJ`, `BESSELI`, `MDETERM`, `INDEX`, `MATCH`, `VLOOKUP`, `HLOOKUP`, `BETA.DIST`, `T.DIST`, `T.DIST.RT`, `T.DIST.2T`, `TDIST`, `F.DIST`, `F.DIST.RT`, `FDIST`, `T.TEST`, `TTEST`, `F.TEST`, `FTEST`, `CONFIDENCE.T`, `PMT`, `FV`, `PV`, `NPER`, `RATE`, `NPV`, `IRR`, `EDATE`, `EOMONTH`, `WEEKDAY`, `WORKDAY`, `NETWORKDAYS`, `DATEDIF`, `FREQUENCY`, `LINEST`, `TREND`, `MODE.MULT`, `IPMT`, `PPMT`, `CUMIPMT`, `CUMPRINC`, `XNPV`, `XIRR`, `MIRR`, `TEXT`, `CONVERT`, `PRICE`, `YIELD`, `DURATION`, `MDURATION`, `DSUM`, `DAVERAGE`, `DCOUNT`, `DCOUNTA`, `DMIN`, `DMAX`, `SORT`, `UNIQUE`, `FILTER`, `XLOOKUP`, `XMATCH`, `LET`, `INDIRECT`, `OFFSET`, `TEXTBEFORE`, `TEXTAFTER`, `TEXTSPLIT`, `GROWTH`, `LOGEST`, `TAKE`, `DROP`, `CHOOSECOLS`, and `WHATIF` on that same sheet.
Text results are stored as inline strings. `LEN`, the slice functions,
and `FIND` / `SEARCH` count Unicode scalar values. `TRIM` collapses only
the space character U+0020. `SEARCH` ignores ASCII letter case and does
not treat `*` or `?` as wildcards; a miss leaves the stored value.
`SUBSTITUTE` replaces every match, or the one match named by a fourth
1-based count. An empty search text leaves the stored value, and a count
past the last match leaves the text unchanged. `REPT` stops at 32,767
scalar values. `EXACT` writes 1 or 0. `AND` and `OR` take up to 255
comma-separated numbers or comparisons and write 1 or 0. A cell range
is not expanded, and an empty call or a text argument leaves the stored
value. `XOR` uses those same arguments and writes 1 when an odd count of
them is not zero. `TEXTJOIN` takes a delimiter, a number, and the same text
arguments as `CONCAT`, including a cell range. A nonzero number skips
empty text. A missing cell inside a range is empty text. A shared-string
cell contributes its text. A missing cell named on its own leaves the
stored value. The joined text stops at 32,767 scalar values. An empty
value list writes an empty string. `NOT` turns zero into 1 and any other number into 0. `SQRT` of a
negative number leaves the stored value. `POWER` of a negative base with
a non-integer exponent leaves the stored value. `MOD` uses the sign of
the divisor, and a zero divisor leaves the stored value. `SIGN` writes
-1, 0, or 1. `PRODUCT` multiplies the same numeric arguments as `SUM`;
an empty call writes 0, and a non-finite product leaves the stored value.
`QUOTIENT` drops the fraction toward zero. A zero divisor, or a magnitude
at or above 1e15, leaves the stored value. `PI` writes the 64-bit
constant, shown to eight decimal places. `EVEN` and `ODD` round away from
zero; zero stays even, and a magnitude at or above 1e15 leaves the stored
value. `REPLACE` removes a 1-based span of Unicode scalar values and writes
the new text there. A start before 1, or more than one past the end, leaves
the stored value. A count that runs past the end removes only the tail.
`VALUE` reads an optional sign, digits, and one dot. Spaces around the
text are ignored. Thousands separators, exponents, and dates leave the
stored value. `T` keeps text and writes an empty string for a number.
`N` keeps a number and writes 0 for text. `ROUNDUP` rounds away from
zero and `ROUNDDOWN` rounds toward zero. The digit count is truncated
and must be from -10 through 10. `CEILING.MATH` and `FLOOR.MATH` move toward +infinity or -infinity, to a
multiple of the significance. An omitted significance is 1. The sign of the
significance is ignored, and a significance of 0 writes 0. A third number
that is not 0 reverses the direction for a negative number. A non-finite
result or text leaves the stored value. `CEILING` and `FLOOR` take
a significance. The signs must match. A zero significance makes `CEILING`
write 0 and makes `FLOOR` leave the stored value. `CEILING` moves away
from zero and `FLOOR` moves toward zero, to a multiple of that
significance. A magnitude at or above 1e15, or a call with one argument,
leaves the stored value. `MEDIAN` uses the same
numeric arguments as `SUM`, including a cell range. An even count averages
the two middle numbers. An empty call leaves the stored value. `ISNUMBER`
and `ISTEXT` write 1 or 0. A missing cell leaves the stored value. A
shared-string cell is text, so `ISTEXT` writes 1 and `ISNUMBER` writes 0.
`GCD` and `LCM` use those same
numeric arguments. A fraction is dropped toward zero. A negative number,
a magnitude at or above 1e15, or an empty call leaves the stored value.
`LCM` of a zero writes 0. `LN` and `LOG10` need a positive number.
`LOG` uses base 10 when the base is omitted. A base that is not positive,
or is 1, leaves the stored value. `EXP` leaves the stored value when the
result is not finite. These results are binary floating point, shown to
eight decimal places. `FACT` drops the fraction toward zero. A negative
number, or 171 and above, leaves the stored value. Above 22 the product
is no longer an exact integer. `SIN`, `COS`, and `TAN` take radians.
`RADIANS` and `DEGREES` convert a number. A non-finite result leaves the
stored value. `ASIN` and `ACOS` need a number from -1 through 1 and write
radians. `ATAN` writes radians. `ATAN2` takes the x coordinate first, then
y, and leaves the stored value when both are zero. `SINH`, `COSH`, and
`TANH` leave the stored value when the result is not finite. `COMBIN`
drops the fraction toward zero. A negative number, a second number larger
than the first, or a first number at or above one million leaves the
stored value. While the result is below 1e15 it is rounded to an integer. `PERMUT` uses
the same limits and does not repeat items, so the second number cannot be
larger than the first. `PERMUTATIONA` allows repetition. Zero to a positive
power is 0, and zero to the power 0 is 1. `LARGE` and `SMALL` use the same
numeric arguments as `SUM`, and the last argument is a 1-based rank. The
rank is truncated toward zero. A rank below 1 or past the last number
leaves the stored value. `TRUNC` drops the fraction toward zero, the same
way as `ROUNDDOWN`. The digit count defaults to 0 and must be from -10
through 10. `ISEVEN` and `ISODD` write 1 or 0. The fraction is dropped
toward zero, unlike `EVEN` and `ODD`, which round away from zero. Zero is
even. A magnitude at or above 1e15, or text, leaves the stored value.
`CODE` and `CHAR` use Unicode scalar values. `CODE` reads the first scalar.
A number is turned into text the same way a calculated number is shown, and
an empty text leaves the stored value. `CHAR` drops the fraction toward
zero. Code 0, a surrogate, or a value above 1114111 leaves the stored
value. `COUNTA` counts stored numbers and non-empty text, including a shared
string. `COUNTBLANK` counts the rest, including an empty inline string. A
missing cell inside a range counts as blank. A missing cell named on its
own leaves the stored value. An index past the shared-string table counts
as blank. An empty
call writes 0. `IFERROR` takes two arguments. When the first cannot be
calculated, or is not a finite number, the second is written. The second
is left unread when the first succeeds. A third argument, or a second
that also fails, leaves the stored value. `CLEAN` removes characters below
U+0020. `PROPER` uppercases the first Unicode letter of each word and
lowercases the rest. A character that is not a letter starts a new word.
`CHOOSE` takes a 1-based index and up to 254 values. The index is
truncated toward zero. Only the chosen value is calculated. An index
below 1, an index past the last value, or a chosen value that cannot be
calculated leaves the stored value. `SWITCH` compares one value with up to
126 later values. Numbers match within 1e-9. Text matches exactly. A
number does not match text. The last argument is the default when the
call has an even count. Only the chosen result is calculated. No match
and no default, or a chosen result that cannot be calculated, leaves the
stored value. `IFS` takes up to 127 condition and value pairs. The first
nonzero number selects its value, and later pairs are left unread. A text
condition, an odd argument count, no match, or a chosen value that cannot
be calculated leaves the stored value. `BITAND`, `BITOR`, and `BITXOR` drop
the fraction toward zero. A negative number, or a magnitude at or above
2^48, leaves the stored value. `BITLSHIFT` and `BITRSHIFT` use that same
number. The shift count is truncated toward zero, and a negative count
shifts the other way. A count past 53, or a result at or above 2^48,
leaves the stored value. `MROUND` rounds to the nearest multiple. A tie
moves away from zero. The signs must match. A zero multiple leaves the
stored value, and `MROUND` of zero and zero writes 0. A magnitude at or
above 1e15 leaves the stored value. `SUMIF` takes one cell range and a
criterion. The criterion is a number, or text that starts with `=`,
`<>`, `>=`, `<=`, `>`, or `<` and then a number. Numbers match within
1e-9. Only stored numbers are added. Text cells, blank cells, and
shared-string cells are skipped. No match writes 0. Other text without `*`, `?`, or `~`, a third
argument, or a call that does not start with a range leaves the stored
value. A criterion containing `*`, `?`, or `~` matches text. The match
ignores ASCII case. `~` escapes the next character. `*` and `?` count
Unicode scalar values. A pattern longer than 64 scalar values, or a cell
longer than 256, leaves the stored value. A wildcard does not match a
number. `SUMIF` still adds only numbers, so that criterion writes 0.
`COUNTIF` counts the text cells that match. `AVERAGEIF` with no matching
number leaves the stored value. `=c*` and `<>c*` use the same pattern.
`>apple` still leaves the stored value. `SUMIFS`, `AVERAGEIFS`, `MINIFS`,
and `MAXIFS` accept that same one wildcard on their criteria range and
still read numbers from the value range. There is no second criteria pair.
`COUNTIF` uses that same range and criterion and writes how many stored
numbers match. A wildcard counts text cells instead. No match writes 0.
`AVERAGEIF` uses that same range and criterion and writes the average.
No match leaves the stored value. `SUMPRODUCT` multiplies equal-sized cell ranges and adds the
products. One range is a sum. A blank cell, a text cell, or a shared-string
cell counts as 0. Up to 8 ranges are read. A different size, a ninth
range, or an argument that is not a range leaves the stored value.
`MINIFS` and `MAXIFS` take a value range, one criteria range of the same
size, and one criterion of the same kind. Only stored numbers are
considered. No match writes 0. A different size, other text without a
wildcard, or a second criterion leaves the stored value. `SUMIFS` adds
those same matching numbers. No match writes 0. A different size, other
text without a wildcard, or a second criterion leaves the stored value.
`AVERAGEIFS` writes the average of those same matching numbers. No match
leaves the stored value. A different size, other text without a wildcard,
or a second criterion leaves the stored value. `SUMSQ` adds the squares of the numbers `SUM` would
read. Text inside a range is skipped. An empty call writes 0. Other text,
or a result that is not finite, leaves the stored value. `STDEV` and
`STDEV.S` write the sample standard deviation, dividing by one less than
the count. Fewer than two numbers leaves the stored value. `STDEVP` and
`STDEV.P` divide by the count. One number writes 0. An empty call leaves
the stored value. Text inside a range is skipped. Other text, or a result
that is not finite, leaves the stored value. `VAR` and `VAR.S` write the
sample variance, dividing by one less than the count. Fewer than two
numbers leaves the stored value. `VARP` and `VAR.P` divide by the count.
One number writes 0. An empty call leaves the stored value. Text inside a
range is skipped. Other text, or a result that is not finite, leaves the
stored value. `AVEDEV` writes the average absolute deviation from the
mean of the numbers `SUM` would read. One number writes 0. An empty call
leaves the stored value. Text inside a range is skipped. Other text, or a
result that is not finite, leaves the stored value. `DEVSQ` adds the
squared deviations from that same mean. One number writes 0. An empty
call leaves the stored value. Text inside a range is skipped. Other text,
or a result that is not finite, leaves the stored value. `GEOMEAN` writes
the geometric mean of the numbers `SUM` would read. Every number must be
positive. One positive number writes that number. An empty call, a zero,
a negative, other text, or a result that is not finite leaves the stored
value. Text inside a range is skipped. `HARMEAN` writes the harmonic
mean of the numbers `SUM` would read. Every number must be positive. One
positive number writes that number. An empty call, a zero, a negative,
other text, or a result that is not finite leaves the stored value. Text
inside a range is skipped. `COUNTIFS` counts stored numbers in one range
with one criterion, the same way `COUNTIF` does, including a wildcard. No
match writes 0. Other text without a wildcard, a second criterion, or a
call that does not start with a range leaves the stored value. `SLOPE` takes a y range and an x range of
the same size. A pair is used when both cells hold finite stored numbers.
Text, blanks, and shared strings are skipped. Fewer than two pairs, a zero
spread in x, a different size, or an argument that is not a range leaves
the stored value. `INTERCEPT` uses those same pairs and writes where the
line crosses y at x = 0. The same failures leave the stored value.
`CORREL` and `PEARSON` write the correlation of those same pairs. Fewer
than two pairs, a zero spread in either range, a different size, or an
argument that is not a range leaves the stored value. `RSQ` writes the
square of that correlation. A negative line still writes a positive
square. The same failures leave the stored value. `FORECAST` and
`FORECAST.LINEAR` take one x and then those same ranges. The result is
the line at that x. The x must be a finite number. The same pair failures
leave the stored value. `STEYX` writes the standard error of the y values
around that line. It needs at least three pairs. Fewer than three pairs,
a zero spread in x, a different size, or an argument that is not a range
leaves the stored value. `COVARIANCE.P` and `COVAR` divide the paired
products by the count. `COVARIANCE.S` divides by one less than the count.
Fewer than two pairs, a different size, or an argument that is not a range
leaves the stored value. A zero spread writes 0. `RANK` and `RANK.EQ` write the rank of one
number in one range. Ties share the better rank. Order 0, or a missing
order, ranks the largest as 1. Any other finite order ranks the smallest
as 1. Text, blanks, and shared strings in the range are skipped. Numbers
match within 1e-9. A missing number, a call without a range, or an extra
argument leaves the stored value. `RANK.AVG` averages the ranks of tied
numbers. A single number keeps the same rank. The same order, match, and
failure rules apply. `PERCENTILE` and `PERCENTILE.INC` take one range and
a k from 0 through 1. k = 0 writes the smallest stored number and k = 1
writes the largest. Text, blanks, and shared strings are skipped. An empty
range, a k outside that span, or a call that does not start with a range
leaves the stored value. `QUARTILE` and `QUARTILE.INC` take that same range
and a quartile number. The number is truncated toward zero and must land
on 0, 1, 2, 3, or 4. Those steps are the smallest number, then 0.25, 0.5,
0.75, and the largest. The same skips and failures leave the stored value.
`MODE` and `MODE.SNGL` write the stored number that appears most often in
one range. A tie writes the number that appears first. Text, blanks, and
shared strings are skipped. Numbers match within 1e-9. If no number
repeats, the call is not a range, or there is an extra argument, the
stored value stays. `PERCENTRANK` and `PERCENTRANK.INC` write where one
number sits in one range, from 0 at the smallest stored number to 1 at the
largest. A value between two numbers is interpolated. At least two numbers
are required. Text, blanks, and shared strings are skipped. A number
outside the range, fewer than two numbers, a call that is not a range, or
an extra argument leaves the stored value. `PERCENTILE.EXC` takes one
range and a k strictly between 0 and 1. The place is k times one more
than the count of stored numbers, and a place between two numbers is
interpolated. A k of 0 or 1, a place below the first number or past the
last, an empty range, or a call that is not a range leaves the stored
value. Text, blanks, and shared strings are skipped. `QUARTILE.EXC` takes
that same range and a quartile number. The number is truncated toward zero
and must land on 1, 2, or 3. Those steps are the exclusive 0.25, 0.5, and
0.75 places. A quartile of 0 or 4, a number outside 1 through 3, or a call
that is not a range leaves the stored value. `PERCENTRANK.EXC` writes
where one number sits in one range, from one divided by one more than the
count at the smallest stored number up to the count divided by that same
total at the largest. It does not write 0 or 1. A value between two numbers
is interpolated. One matching number writes 0.5. Text, blanks, and shared
strings are skipped. A number outside the range, an empty range, a call
that is not a range, or an extra argument leaves the stored value. `STANDARDIZE`
writes (x - mean) / scale for three numbers. A scale of 0 or less, a
non-numeric argument, or a missing argument leaves the stored value. `SKEW`
and `SKEW.P` write the skewness of the numbers `SUM` would read. Both need
at least three numbers. `SKEW` divides by the sample standard deviation.
`SKEW.P` divides by the population standard deviation. A zero spread,
fewer than three numbers, an empty call, or direct text leaves the stored
value. Text inside a range is skipped. `KURT` writes the excess kurtosis of
those same numbers, using the sample standard deviation. At least four
numbers are required. A zero spread, fewer than four numbers, an empty
call, or direct text leaves the stored value. `TRIMMEAN` takes one range and a
fraction from 0 up to, but not including, 1. It drops that fraction of the
stored numbers, the same count from the low end and the high end, rounding
the dropped count down to an even number, then writes the mean of what
remains. A fraction of 0 writes the mean of every stored number. Text,
blanks, and shared strings are skipped. A fraction below 0 or of 1 or more,
an empty range, or a call that is not a range leaves the stored value. `FISHER`
writes half the natural log of (1 + x) / (1 - x). x must be strictly
between -1 and 1. `FISHERINV` writes the inverse for any finite number.
A value on or outside that span, a non-finite result, or text leaves the
stored value. `SQRTPI` writes the square root of a number times pi. A
negative number or text leaves the stored value. `COMBINA` writes how many
ways items can be chosen when repeats are allowed. Both counts are
truncated toward zero and must be at least 0 and below 1,000,000. Choosing
zero writes 1. Choosing from zero items writes 0. A negative count, text,
or a pair whose formula reaches 1,000,000 leaves the stored value. `SUMX2MY2`,
`SUMX2PY2`, and `SUMXMY2` take two ranges of the same length. A pair is
used only when both cells hold finite stored numbers. Text, blanks, and
shared strings skip that pair. `SUMX2MY2` adds the first square minus the
second. `SUMX2PY2` adds the two squares. `SUMXMY2` adds the square of the
difference. No numeric pairs writes 0. A different size, a call that is not
two ranges, or a non-finite square leaves the stored value. `GESTEP` writes 1
when the first number is greater than or equal to the second, and 0
otherwise. A missing second number compares with 0. `DELTA` writes 1 when
the two numbers are exactly equal, and 0 otherwise. A missing second number
compares with 0. A non-finite number or text leaves the stored value.
`MULTINOMIAL` writes the factorial of the sum divided by the factorial of
each count. Counts are truncated toward zero and must be at least 0. Their
sum must stay below 1,000,000. Text inside a range is skipped. An empty
call, a negative count, direct text, or a sum that reaches 1,000,000 leaves
the stored value. `FACTDOUBLE` writes the double factorial. The number is
truncated toward zero. An even number steps down by two to 2, and an odd
number steps down by two to 1. Zero and one write 1. A negative number, a
number of 301 or more, or text leaves the stored value. `POISSON` and
`POISSON.DIST` take an x, a mean, and a third number. x is truncated toward
zero. A third number of 0 writes the probability of that count. Any other
finite third number writes the sum of the probabilities from 0 through x.
x must be at least 0 and below 171. The mean must be at least 0 and below
700. A negative argument, a missing third number, or text leaves the stored
value. `BINOM.DIST` and `BINOMDIST` take a success count, a trial count, a
probability, and a fourth number. The counts are truncated toward zero. A
fourth number of 0 writes the probability of that success count. Any other
finite fourth number writes the sum from 0 through that count. The
probability must be from 0 through 1. The success count must not pass the
trial count, and the trial count must be below 171. A negative count, a
probability outside that span, a missing fourth number, or text leaves the
stored value. `EXPON.DIST` and `EXPONDIST` take an x, a lambda, and a third
number. A third number of 0 writes lambda times e to the power of minus
lambda times x. Any other finite third number writes 1 minus that power of
e. x must be at least 0. Lambda must be greater than 0. A negative x, a
lambda of 0 or less, a missing third number, an exponent that overflows, or
text leaves the stored value. `NEGBINOM.DIST` takes a failure count, a
success count, a probability, and a fourth number. A fourth number of 0
writes the probability of that many failures before the success count. Any
other finite fourth number writes the sum from 0 failures through that
count. `NEGBINOMDIST` takes the same first three numbers and writes that
point probability. Both counts are truncated toward zero. The failure count
must be at least 0 and below 171. The success count must be at least 1 and
below 171. The probability must be greater than 0 and less than 1. A count
outside that span, a probability of 0 or 1, a missing fourth number on
`NEGBINOM.DIST`, or text leaves the stored value. `HYPGEOM.DIST` takes the
successes drawn, the draw size, the successes in the population, the
population size, and a fifth number. A fifth number of 0 writes the
probability of that draw. Any other finite fifth number writes the sum from
the smallest possible draw through that count. `HYPGEOMDIST` takes the same
first four numbers and writes that point probability. Every count is
truncated toward zero, must be at least 0, and must be below 171. The draw
must fit in the population. The drawn successes must fit the draw and the
successes available. The rest of the draw must fit the rest of the
population. A count outside that span, a missing fifth number on
`HYPGEOM.DIST`, or text leaves the stored value. `WEIBULL.DIST` and `WEIBULL`
take an x, an alpha, a beta, and a fourth number. A fourth number of 0
writes the density. Any other finite fourth number writes 1 minus e to the
power of minus (x divided by beta) raised to alpha. x must be at least 0.
Alpha and beta must be greater than 0. A density at 0 when alpha is below
1, a non-positive alpha or beta, a negative x, a missing fourth number, a
power that overflows, or text leaves the stored value. `GAMMA` writes the
gamma function. A positive whole number up to 170 writes the factorial of
the number one below it. Other finite numbers use a Lanczos approximation,
and a negative number uses the reflection formula. A non-positive whole
number, a whole number above 170, a result that overflows, or text leaves
the stored value.
`GAMMALN` writes the natural logarithm of that function for a number greater
than 0. A number that is not greater than 0, a result that overflows, or
text leaves the stored value. `GAMMA.DIST` and `GAMMADIST` take an x, an
alpha, a beta, and a fourth number. A fourth number of 0 writes the density.
Any other finite fourth number writes the cumulative probability. x must be
at least 0. Alpha and beta must be greater than 0. A density at 0 is 0 when
alpha is greater than 1, and 1 divided by beta when alpha is 1. A density at
0 when alpha is below 1 leaves the stored value. A whole alpha writes the
cumulative probability as one minus a Poisson sum through alpha minus 1.
That sum is refused when alpha is 172 or more, or when x divided by beta is
700 or more. A fractional alpha uses a series of at most 200 terms, and a
series that does not settle leaves the stored value. A negative x, a
non-positive alpha or beta, a missing fourth number, or text leaves the
stored value. `BINOM.INV` and `CRITBINOM` take a trial count, a probability,
and a criterion. They write the smallest success count whose cumulative
binomial probability is at least the criterion. The trial count is truncated
toward zero, must be at least 0, and must be below 171. The probability and
the criterion must be greater than 0 and less than 1. A count outside that
span, a probability or criterion of 0 or 1, a missing third number, or text
leaves the stored value. `CHISQ.DIST` takes an x, a degree count, and a
third number. The degree count is truncated toward zero and must be at least
1. A third number of 0 writes the density of a gamma distribution whose
shape is half the degrees and whose scale is 2. Any other finite third
number writes that cumulative probability. `CHISQ.DIST.RT` and `CHIDIST`
take an x and a degree count and write 1 minus that cumulative probability.
An even degree count uses the Poisson sum on a cumulative call. That sum is
refused when the degrees are 344 or more, or when x is 1400 or more. An odd
degree count uses a series of at most 200 terms, and a series that does not
settle leaves the stored value. A negative x, a degree count below 1, a
missing third number on `CHISQ.DIST`, or text leaves the stored value.
`NORM.S.DIST` takes a z and a second number. A second number of 0 writes the
standard normal density. Any other finite second number writes the cumulative
probability. `NORMSDIST` takes one number and writes that cumulative
probability. The cumulative value is one half times one plus or minus the
lower gamma series for shape 1/2 and z squared over 2, using at most 200
terms. A series that does not settle leaves the stored value. A density whose
exponential underflows writes 0. A missing second number on `NORM.S.DIST`, or
text, leaves the stored value. `NORM.DIST` and `NORMDIST` take an x, a mean,
a scale, and a fourth number. A fourth number of 0 writes the standard normal
density of (x minus the mean) divided by the scale, then divided by the scale.
Any other finite fourth number writes the cumulative standard normal
probability of that same ratio. The scale must be greater than 0. The
cumulative call uses the same 200-term series as `NORM.S.DIST`. A scale that
is not greater than 0, a series that does not settle, a missing fourth number,
or text leaves the stored value. `ERF` with one number writes the error
function from 0 to that number. With two numbers it writes the error
function of the second minus the error function of the first. `ERFC` writes
1 minus the error function. `GAUSS` writes the standard normal cumulative
probability minus 1/2. `PHI` writes the standard normal density. The error
function and `GAUSS` use the same 200-term gamma series as `NORM.S.DIST`. A
series that does not settle, or text, leaves the stored value.
`LOGNORM.DIST` takes an x, a mean, a scale, and a fourth number. A fourth
number of 0 writes the standard normal density of the natural log of x, after
subtracting the mean and dividing by the scale, then divided by x times the
scale. Any other finite fourth number writes that cumulative probability.
`LOGNORMDIST` takes the first three numbers and writes the cumulative
probability. x and the scale must be greater than 0. The cumulative call uses
the same 200-term series as `NORM.S.DIST`. An x or scale that is not greater
than 0, a series that does not settle, a missing fourth number on
`LOGNORM.DIST`, or text leaves the stored value. `BINOM.DIST.RANGE` takes a
trial count, a probability, a lower success count, and an optional upper
count. It writes the sum of the point probabilities from the lower count
through the upper count. A missing upper count uses the lower count. Every
count is truncated toward zero. The trial count must be at least 0 and below
171. The probability must be from 0 through 1. The upper count must be at
least the lower count, and neither count may pass the trial count. A value
outside that span, a missing lower count, or text leaves the stored value.
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
the displayed value, a shape the gamma cumulative refuses, or text leaves
the stored value.
`ROMAN` writes the classic Roman form of a number from 1 through 3999. The
fraction is dropped toward zero. A form argument other than 0 leaves the
stored value. `ARABIC` reads that same classic form, ignoring ASCII case,
and writes the number. A number outside 1 through 3999, a form other than
0, text that is not that classic form, or text where a number is required
leaves the stored value. `SECH`, `CSCH`, and `COTH` write the hyperbolic
secant, cosecant, and cotangent. A zero denominator, a non-finite result, or
text leaves the stored value. `ACOT` writes the inverse cotangent in radians,
from 0 through pi. Zero writes pi over 2, and a negative number adds pi to
the arctangent of its reciprocal. `ACOTH` writes the inverse hyperbolic
cotangent for a number whose absolute value is greater than 1. A number on
the closed span from -1 through 1, a non-finite result, or text leaves the
stored value. `CHISQ.TEST` and `CHITEST` take two colon ranges of the same
shape. They write the right-tail chi-square probability of the sum of
(actual minus expected) squared, divided by expected. A single row uses one
less than the column count as the degrees of freedom. A single column uses
one less than the row count. A rectangle uses the product of one less than
each side. A single cell, ranges of different shape, an expected value that
is not greater than 0, a blank or text cell, a call that is not two colon
ranges, or a right tail the chi-square helper refuses leaves the stored
value. `AVERAGEA`, `MINA`, `MAXA`, `STDEVA`, and `VARA` read numbers and
count text as 0. A blank cell is skipped. A shared string counts as text,
so it is 0. `STDEVA`
and `VARA` are the sample standard deviation and variance and need at least
two counted values. `MINA` and `MAXA` write 0 when nothing is counted. An
empty `AVERAGEA`, `STDEVA`, or `VARA` call, a non-finite number, or a direct
call the reader cannot parse leaves the stored value. `DATE` writes an Excel
1900 serial number. A year from 0 through 1899 has 1900 added to it. Month
and day overflow into the neighboring months. 29 February 1900 is serial
60 even though that day did not occur. `YEAR`, `MONTH`, and `DAY` read a
serial from 0 through 2958465. Serial 0 is 1900-01-00. A negative serial, a
year outside 0 through 9999, a month or day whose magnitude reaches 1e9, or
text leaves the stored value. `DEC2BIN`, `DEC2HEX`, and `DEC2OCT` truncate
toward zero and write two's-complement text of at most 10 characters.
`DEC2BIN` accepts −512 through 511, `DEC2HEX` accepts −2^39 through 2^39−1,
and `DEC2OCT` accepts −2^29 through 2^29−1. A negative number always uses 10
characters. An optional places argument is an integer from 1 through 10 and
pads a non-negative result with zeros; a places value other than 10 on a
negative number, or a places value shorter than the digits, leaves the stored
value. `BIN2DEC`, `HEX2DEC`, and `OCT2DEC` read at most 10 characters. Hex
letters ignore ASCII case. A full 10-character string whose high bit is set
is negative. `BASE` writes an uppercase digit string for a number from 0 up
to, but not including, 2^53 and a radix from 2 through 36. An optional
minimum length from 0 through 255 pads with zeros and leaves the stored value
when it is shorter than the digits. `DECIMAL` reads that same text back when
the result stays below 2^53 and the text is at most 255 characters. An empty
string, an invalid digit, or text where a number is required leaves the
stored value. `BESSELJ` and `BESSELI` take the value first and then an order.
The order is truncated toward zero and must be from 0 through 40. The
absolute value must stay below 40. The series stops within 200 terms; if it
does not settle, the stored value stays. `MDETERM` reads one square range of
at most 10 by 10 on the same sheet. A blank cell counts as 0. Text,
including a shared string, a range that is not square, a side longer than 10, or a
non-finite determinant leaves the stored value. A pivot whose absolute value
is at most 1e-12 makes the determinant 0. `INDEX`, `MATCH`, `VLOOKUP`, and
`HLOOKUP` read one range on the same sheet. A number matches only a number,
with `==`. Text ignores ASCII case. An exact match accepts `*`, `?`, and `~`
in the lookup text. `*` and `?` count Unicode scalar values. A pattern longer
than 64 scalars, or a cell longer than 256, leaves the stored value.
Approximate match does not use wildcards. A range is read from its top-left cell. `MATCH` type 0 is exact. An omitted
type, or type 1, walks an ascending row or column and keeps the last key that
is less than or equal to the lookup, then stops at the first greater key.
Type −1 does that for a descending range and a key greater than or equal to
the lookup. `VLOOKUP` and `HLOOKUP` do the ascending walk when the range
lookup is omitted or is any number other than 0. A range lookup of 0 is exact.
A blank cell, or a number beside text, stops an approximate walk. A shared
string is text. `INDEX` row and column numbers are 1-based and truncated
toward zero. An omitted column is accepted only when the range has one column.
A row or column of 0, a position past the range, or a value that is not found
leaves the stored value. A blank cell writes 0 when it is the returned cell.
A shared string is returned as its text. `MATCH` skips a blank on an exact
match, so a blank matches neither 0 nor an empty string. A shared string can
match text. `BETA.DIST` reads x, alpha, beta, and a
cumulative flag on the unit interval. Alpha and beta must be greater than 0.
A cumulative flag of 0 writes the density; any other finite number writes the
regularized incomplete beta. The continued fraction stops within 200 steps,
and a fraction that does not settle leaves the stored value. A fifth or sixth
argument, the distribution bounds, is not read. `T.DIST` reads x, degrees of freedom, and a cumulative
flag. Degrees of freedom must be greater than 0. A flag of 0 writes the
density; any other finite number writes the cumulative distribution.
`T.DIST.RT` writes the right tail. `T.DIST.2T` and `TDIST` require x to be at
least 0. `TDIST` tails must be 1 or 2. They use the same continued fraction,
so a fraction that does not settle leaves the stored value. `F.DIST` reads x, two degrees of freedom,
and a cumulative flag. Both degrees of freedom must be greater than 0, and x
must be at least 0. A flag of 0 writes the density; any other finite number
writes the cumulative distribution. `F.DIST.RT` and `FDIST` write the right
tail. They use the same continued fraction. `T.TEST` and `TTEST` compare two ranges on the same sheet.
Tails must be 1 or 2. The probability uses the absolute statistic: tails 1 is
the upper tail, and tails 2 is twice that, capped at 1. Type 1 is paired and
the ranges must have the same length. A pair is kept only when both cells are
finite numbers, and at least two pairs are required. Type 2 assumes equal
variance and type 3 is the Welch test. Each range then needs at least two
finite numbers. Text and blank cells are skipped. Any other type, a sample
variance of 0, or a fraction that does not settle leaves the stored value.
`F.TEST` and `FTEST` are the two-tailed comparison of those sample variances,
twice the smaller tail and capped at 1. A sample variance of 0 leaves the
stored value. `CONFIDENCE.T` reads alpha, a standard deviation, and a sample
size. Alpha must be greater than 0 and less than 1, the standard deviation
must be greater than 0, and the size is truncated to an integer from 2 through
1000000. The critical value is a bisection of at most 80 steps; if it does not
settle, the stored value stays. `PMT`, `FV`, `PV`, and `NPER` use the ordinary
annuity. The number of periods must be greater than 0 and at most 1e6. A rate
of −1 or below leaves the stored value. A rate of 0 uses the linear form. An
omitted future value or present value is 0. An omitted type is 0, a payment at
the end of the period; any other finite type is a payment at the beginning.
`RATE` starts at guess 0.1 when the guess is omitted and takes at most 40
Newton steps. `IRR` does the same on a colon range of 2 through 128 numbers.
The first `IRR` value is time 0. A blank cell counts as 0. Text, including a
shared string, leaves the stored value. `NPV` discounts the following numbers
from period 1, so it does not include a time-0 payment. A rate that does not
settle, or a non-finite result, leaves the stored value. `IPMT` and `PPMT` use that same annuity. The period is truncated toward zero and must be from 1 through the number of periods. A payment at the beginning makes the first `IPMT` 0. `CUMIPMT` and `CUMPRINC` add those payments from the start period through the end period, with a future value of 0. The span is at most 10000 periods. `XNPV` and `XIRR` read two colon ranges of the same length, 1 through 128 numbers for `XNPV` and 2 through 128 for `XIRR`. A blank or text cell leaves the stored value. The year length is 365. A date before the first date leaves the stored value. `XIRR` needs one positive value and one negative value, dates that do not go backward, and at most 40 Newton steps from guess 0.1. `MIRR` reads one colon range of 2 through 128 numbers, a finance rate, and a reinvest rate. A zero is neither a deposit nor a return. A range that is all deposits or all returns leaves the stored value. `TEXT` reads a value and a format of at most 64 characters. Up to three sections, split on a semicolon that is not inside quotes: positive, negative, and zero. A fourth section, a `[` condition, or a color code leaves the stored value. An empty section writes an empty string. The negative section is applied to the absolute value, so a minus sign has to be quoted text in that section. With two sections, zero uses the first. A text value is accepted only by a single `@` section. `@` writes the value as text, and a number is shown the same way a calculated number is shown. A number format uses `0`, `#`, one dot, a comma that turns on thousands separators, and one `%` that multiplies by 100. Extra integer digits are kept. `#` after the dot drops trailing zeros, and `0` keeps them. A negative number keeps a leading minus, including `-0.00` when decimal places remain. A date format uses `yyyy`, `yy`, `mm`, `m`, `dd`, and `d` on the 1900 serial, with `-`, `/`, `.`, space, and `:` between them. Quoted text is copied through. A format that mixes a date token with a number token, or a time, scientific, fraction, or color code, leaves the stored value. `CONVERT` reads a number and two unit names. The names are case-sensitive. Length is `m`, `cm`, `mm`, `km`, `in`, `ft`, `yd`, and `mi`. Mass is `g`, `kg`, `mg`, `lbm`, and `ozm`. Time is `sec`, `mn`, `hr`, `day`, and `yr`, and a year is 365.25 days. Temperature is `C`, `F`, and `K`. A temperature below absolute zero, two units from different groups, or an unknown name leaves the stored value. Prefixes on an arbitrary unit are not read. `EDATE` and `EOMONTH` move a serial by a
whole number of months on that same 1900 system. The day is kept when the
target month has it, and otherwise becomes the last real day of that month.
29 February 1900 is not treated as the end of February. The serial must be at
least 1, and the month shift must stay within 120000. `WEEKDAY` return type 1
numbers Sunday as 1, type 2 numbers Monday as 1, and type 3 numbers Monday as
0. An omitted type is 1. Any other type leaves the stored value. `DATEDIF`
accepts `Y`, `M`, `D`, `MD`, `YM`, and `YD`. An end before the start leaves
the stored value. `NETWORKDAYS` and `WORKDAY` count Monday through Friday. An
optional holiday range may hold at most 512 numbers; text in that range is
skipped. A weekend holiday is not counted twice. The inclusive span must be at
most 100000 days, and `WORKDAY` moves at most 10000 working days. When `workbookPr` has `date1904="1"` or `date1904="true"`, serial 0 is 1904-01-01. `DATE`, `YEAR`, `MONTH`, `DAY`, `EDATE`, `EOMONTH`, `DATEDIF`, and a `TEXT` date format shift that serial by 1462 onto the 1900 calendar. `WEEKDAY`, `NETWORKDAYS`, and `WORKDAY` shift by 1461, so the fictional 29 February 1900 does not move those weekdays. A date before 1904-01-01 leaves the stored value. A holiday before that day is skipped. Without the flag, the 1900 system is unchanged. `IF` stays
numeric. A shared-string cell on the sheet being recalculated is read as its
shared text. An index past the table is blank. A reference `Sheet!A1` or
`'My Sheet'!A1` reads one cell from that pass. The sheet name ignores ASCII case, and
a quoted name is at most 31 Unicode scalar values. A doubled apostrophe inside
a quoted name is one apostrophe. An unquoted name is the same kind of token as
a function name. Before the edited sheet is recalculated, every sheet is passed once in workbook
order. Each formula on that pass reads stored values on its own sheet,
including the stored value of another formula there, and it reads an earlier
sheet after that sheet's pass. At most 4096 formulas on a sheet are passed. A
formula that returns nothing keeps its stored value. A spill formula keeps its
stored value. The pass is not written back into the other sheet's file. A
qualified reference on the sheet being edited reads that pass, so `Budgets!A1`
is the pass and a bare `A1` still follows a live formula. `INDIRECT` reads one text address in A1 style, at most 128 characters. `$` is ignored. A sheet name, including the sheet being edited, is a qualified reference and reads that one pass. Without a sheet name it is a bare reference on the sheet being edited. A range is `A1:B2`. `R1C1`, brackets, and another workbook leave the stored value. Used as one value, it must be one cell. It expands where a colon range expands, and as a whole argument of `SUM`, `AVERAGE`, `MIN`, `MAX`, `COUNT`, `PRODUCT`, `AVERAGEA`, `COUNTA`, and `COUNTBLANK`. `OFFSET(reference, rows, cols, height, width)` starts at one cell, a colon range, or `INDIRECT`. `rows` and `cols` are truncated and must be from -1000000 through 1000000. `height` and `width` default to the reference size, are truncated, and must be from 1 through 256. The result is at most 4096 cells and must stay on the sheet. Used as one value, it must be one cell. A result that leaves the sheet leaves the stored value. `TEXTBEFORE` and `TEXTAFTER` take text, a literal delimiter, and an optional instance. The match is case-sensitive and counts Unicode scalar values. The instance defaults to 1, is truncated, and must be from 1 through 16. There is no match mode, no wildcard, and no value to use when the delimiter is missing. A missing delimiter, an empty delimiter, or text longer than 1024 scalar values leaves the stored value. A delimiter longer than 64 scalar values does the same. `TEXTSPLIT` is recalculated only when the formula is that one call. It takes one column delimiter and writes the pieces downward, at most 16, into existing value cells. A second delimiter, more than 16 pieces, or a blocked cell leaves the stored value. Consecutive delimiters keep an empty piece. An expression around `TEXTSPLIT` keeps the stored value. An array constant `{1,2;3,4}` is a whole argument of `SUM`, `AVERAGE`, `MIN`, `MAX`, `COUNT`, `PRODUCT`, `AVERAGEA`, `COUNTA`, or `COUNTBLANK`. A comma starts another column and a semicolon starts another row. Each row must have the same number of columns. A value is a plain number, with an optional sign, or quoted text. There is no cell reference, no nested brace, no blank, and no scientific notation. Text is skipped by the numeric functions and counts as 0 for `AVERAGEA`. At most 4096 values. A ragged array, or the constant used as one value or as a `SUMIF` range, leaves the stored value. A cross-sheet range and another workbook are not read. A formula can read `Sheet1:Sheet2!A1` or `Sheet1:Sheet2!A1:B2`. The sheets are the inclusive span in workbook order, at most 32 sheets and 4096 cells. Every sheet in the span is read from that same pass, including the sheet being edited. A missing sheet name leaves the stored value. The reference expands in `SUM`, `AVERAGE`, `MIN`, `MAX`, `COUNT`, `PRODUCT`, `AVERAGEA`, `COUNTA`, `COUNTBLANK`, and wherever a colon range is accepted. `CONCAT`, `CONCATENATE`, and `TEXTJOIN` do not expand it. A circular reference is recalculated only when `calcPr` has `iterate="1"` or `iterate="true"`. `iterateCount` is clamped to 1 through 100 and defaults to 100. `iterateDelta` defaults to 0.001. A negative or non-finite delta becomes 0.001, and a larger delta is cut to 1. A reference that is already being calculated reads the previous pass, or the stored number on the first pass, or 0 when that cell has no stored number. The last pass is written even when the change is still larger than the delta. Without the flag, a cycle leaves the stored value. A workbook defined name is one cell or one colon range on one sheet in this workbook. The name is ASCII case-insensitive, at most 255 characters, and uses letters, digits, underscores, and dots. It must not look like a cell address. A name with `localSheetId` belongs to the sheet at that zero-based position in workbook order. On that sheet it hides a workbook name with the same spelling. On every other sheet it is not visible. A `localSheetId` that is not a sheet index is ignored. A name on the sheet being edited reads those cells the same way a bare reference does, so a formula there is recalculated. A name on another sheet reads the stored snapshot and does not follow a formula. The name expands when it is a whole argument of `SUM`, `AVERAGE`, `MIN`, `MAX`, `COUNT`, `PRODUCT`, `AVERAGEA`, `COUNTA`, or `COUNTBLANK`, and wherever a colon range is accepted, including `SUMIF` and `SUMPRODUCT`. `CONCAT`, `CONCATENATE`, and `TEXTJOIN` do not expand a name. A multi-cell name used as a single value, or inside an expression, leaves the stored value. A name may also be one expression of at most 256 characters. Every cell reference in it must be fully absolute and written after a sheet name, such as `Budgets!$A$1`. Those cells are read from that same one pass. The expression cannot use another name. A relative reference, a 3D reference, another workbook, or a structured table reference is ignored. A formula name is one value and does not expand as a range. A formula can read one column of a table as `Table1[Amount]`. The table name and the column name are ASCII case-insensitive. The column is the data cells under that header, not the header and not a totals row. `headerRowCount` must be missing or 1. `totalsRowCount` must be missing, 0, or 1. On the sheet being edited those cells are read live. On another sheet they are read from the stored snapshot. The column expands where a colon range expands. A column used as one value works only when it has a single data cell. `Table1[[#This Row],[Amount]]`, `#All`, `#Headers`, `#Data`, and `#Totals` are ignored. A shared formula on the sheet being edited is the cell whose `<f t="shared">` contains the formula. Each other cell with the same `si` and an empty shared formula uses that formula shifted by the row and column distance from the master. A `$` keeps that column or row fixed. Text inside quotes is not shifted. A shift that would leave the sheet, a missing master, or a formula type other than shared, including an array formula, leaves the stored value. Shared formulas on other sheets are not expanded. `PRICE`, `YIELD`, `DURATION`, and `MDURATION` use basis 0 through 4. Basis 0 is the US 30/360 day count. Basis 1 counts the actual serial days in the coupon period. Basis 2 counts those same actual days and divides by 360 over the frequency. Basis 3 divides by 365 over the frequency. Basis 4 is European 30/360: a day of 31 becomes 30 on both dates. Frequency is 1, 2, or 4. Any other basis or frequency leaves the stored value. There is no odd first or last coupon. Settlement on or after maturity leaves the stored value. `YIELD` takes at most 40 Newton steps, starting from the coupon rate, or from 0.05 when the coupon is 0. A yield less than or equal to the negative frequency leaves the stored value. `DURATION` and `MDURATION` redeem at 100. `MDURATION` divides the Macaulay duration in years by one plus the yield per coupon period. `DSUM`, `DAVERAGE`, `DCOUNT`, `DCOUNTA`, `DMIN`, and `DMAX` take a database range with a header row, a field, and a criteria range of exactly two rows. The field is a header name or a column number starting at 1. The first header with that spelling is used. A criteria header must match a database header, ignoring ASCII case. A blank criteria cell, or a criteria column whose header is blank, matches every row. A number matches that number. Text is an exact ASCII case-insensitive match, or a numeric comparison when it begins with `=`, `<>`, `<`, `>`, `<=`, or `>=`. A numeric comparison matches only numbers. `=` and `<>` with text that is not a number compare text. A text criterion may contain `*`, `?`, and `~`. `*` and `?` match text, ignoring ASCII case, and count Unicode scalar values. `~` escapes the next character. A pattern longer than 64 scalar values, or a cell longer than 256, leaves the stored value. A wildcard does not match a number. `=` and `<>` may carry the same pattern. A text ordering such as `>apple`, or more than one criteria row, leaves the stored value. `DSUM` of no numbers is 0. `DAVERAGE`, `DMIN`, and `DMAX` with no numbers leave the stored value. Text in the field column is skipped by the numeric functions and counted by `DCOUNTA`. `FREQUENCY`, `LINEST`, `TREND`, and `MODE.MULT` are recalculated only when the formula is that one call. A larger expression keeps the stored value. `FREQUENCY` takes a data range of at most 256 cells and a bin range of 1 through 16 numbers. Bins must be non-decreasing. A blank in the data is skipped. Text, including a shared string, leaves the stored value. A blank or text bin, or a bin that falls, leaves the stored value. The first count is the formula cell. Later counts go down into existing value cells. A missing cell, a formula, or a text cell stops the rest, and those cells keep their previous values. The last count is everything above the last bin. `MODE.MULT` uses one range of at most 256 cells. Text, blanks, and shared strings are skipped. Numbers match within 1e-9. Every value that ties for the highest count, when that count is at least 2, is written downward in the order it first appears. More than 16 modes, or no repeated number, leaves the stored value. The same stop rule applies. `LINEST` takes one column or row of y values and one column or row of x values, the same length, at most 256 cells. A blank or text pair is skipped. At least two finite pairs are required. The slope is written in the formula cell and the intercept in the cell to the right, when that cell exists and is a value cell. Otherwise nothing is written. A constant of 0, which would force the line through the origin, leaves the stored value. Any other finite constant keeps the intercept. A statistics argument of 0, or no statistics argument, keeps the two-cell result. Any other finite statistics argument writes a 5 by 2 grid: slope and intercept, their standard errors, r² and the standard error of y, F and degrees of freedom, then the regression and residual sums of squares. That grid needs at least three finite pairs. A perfect fit, where the residual sum of squares is 0, leaves the stored value. Every cell in the grid must already exist and be a value cell, or nothing is written. There is still one x variable. `TREND` takes those same y and x ranges and a new-x range of 1 through 16 numbers in one column or row. Each new x must be a finite number. Predictions are intercept plus slope times x, first in the formula cell and the rest downward. If any of those cells is missing, a formula, or text, nothing is written. `GROWTH` and `LOGEST` use that same one x, and every y that is used must be greater than 0. `GROWTH` writes predictions the same way `TREND` does: the exp of the intercept plus the slope times x, where the line is fit to ln(y). A fourth argument leaves the stored value. `LOGEST` writes m and b, the exp of that slope and intercept, the same way `LINEST` writes its two cells. A statistics argument other than 0 writes the `LINEST` grid of ln(y), with the first row replaced by m and b. A constant of 0, a perfect fit on that grid, or a non-positive y leaves the stored value. There is no `FORECAST.ETS`. `SORT`, `UNIQUE`, and `FILTER` are recalculated only when the formula is that one call. Each takes one column of at most 256 cells. A blank is skipped. Text, including a shared string, leaves the stored value. Results are written downward into existing value cells, and if any of those cells is missing, a formula, or text, nothing is written. `SORT` order defaults to 1. An order of -1 reverses it. Any other order leaves the stored value. `UNIQUE` keeps the first number and treats values within 1e-9 as the same. `FILTER` takes a second column of the same height and one numeric criterion, the same kind `SUMIF` accepts. A row is kept when that criterion matches. No remaining number leaves the stored value. `TAKE`, `DROP`, and `CHOOSECOLS` are recalculated only when the formula is that one call. The source is one block of numbers, at most 256 rows and 16 columns, and every cell must be a finite number. A blank or text leaves the stored value. Counts are truncated and must be from -256 through 256. `TAKE` keeps that many rows, and an optional second count keeps that many columns. A count of 0, or a count larger than that side, leaves the stored value. A positive count starts at the beginning and a negative count starts at the end. Omitted columns means every column. `DROP` removes that many rows, and an optional second count removes that many columns. A count of 0 keeps that side. A count that leaves nothing leaves the stored value. `CHOOSECOLS` takes one 1-based column index. A negative index counts from the end. A second index, 0, or an index past the block leaves the stored value. The result is at most 256 cells and is written into existing value cells. If any of those cells is missing, a formula, or text, nothing is written. An expression around the call leaves the stored value. `WHATIF(formula_cell, input_cell, inputs)` is recalculated only when the formula is that one call. Both cells are on the sheet being edited, and they must be different. The formula cell must already contain a formula. The inputs are one column of 1 through 16 finite numbers. A blank, text, or a second column leaves the stored value. Each input replaces the input cell, and the formula cell is calculated with that number. Only a finite number is kept. Text, a failed formula, or a cycle leaves the stored value. Results are written downward into existing value cells, and if any of those cells is missing, a formula, or text, nothing is written. This is not an Excel data table. A second input is `WHATIF(formula_cell, row_input, row_values, col_input, col_values)`. The row values are one column of 1 through 8 numbers and the column values are one row of 1 through 8 numbers. The result is at most 64 cells, written across and then down. The two inputs must differ from each other and from the formula cell. An expression around the call leaves the stored value. `XLOOKUP` and `XMATCH` take one row or one column of at most 256 cells. The match is exact and ASCII case-insensitive. There are no wildcards: a lookup value that contains `*`, `?`, or `~` leaves the stored value. Blank cells are skipped. The first match wins. The `XLOOKUP` return range must be the same length. A blank return cell is 0. The optional fourth argument is written when nothing matches. Without it, or when `XMATCH` finds nothing, the stored value remains. `XMATCH` accepts match mode 0 or no mode. Any other mode leaves the stored value. `LET` binds up to eight names, then calculates the last argument. A name starts with a letter or underscore and uses letters, digits, underscores, and dots. It must not look like a cell address. A later binding replaces an earlier one with the same spelling. Names from the workbook are not visible inside `LET`. Cell references and functions still are. There is no `LAMBDA`. Other formulas keep their stored value. Drawings are copied through. A blank cell that the
table only filled in so the columns line up is not inserted. PowerPoint
(`.pptx`, `.pptm`, `.ppsx`) → a read-only HTML preview (one card per
slide, including speaker notes).
`.xlsb` stays in the archive browser. Misnamed `.docx` sheets or slides
follow the sniff, not the extension.

Catalog **Document Editor** creates `Untitled.orchid` (linked when a
`ChunkStore` is available). **Media Player** is a file picker → media Viewer.

## Images

Wide format set (JPEG/PNG/WebP/RAW/SVG/HEIC via WIC, …). Zoom/pan, folder
playlist, thumbs, slideshow, Timeline/Map/Calendar, EXIF, sibling-file
edits. **Files → Photos** can ask Windows for face rectangles and groups
`people/` tags. The image viewer draws those stored rectangles on the
decoded picture. A rotated or flipped view hides them. The boxes do not
name the person. The view stays 8-bit RGBA.
Radiance HDR and OpenEXR are tone-mapped into that buffer (Reinhard, then
sRGB) so pixels brighter than 1.0 are not clipped to white. The status
line marks those files `tone-mapped`.

## PDF

Needs `pdfium.dll`. Page nav, fit width / page, zoom, outline sidebar,
in-document find (`Ctrl+F`, match case), print (`Ctrl+P`).

Drag to select text (double-click a word, `Ctrl+A` the page). `Ctrl+C`
copies the selection, or the whole page when nothing is selected.
**Highlight** writes highlight annotations into the open file (reloads so
you can stack marks). If that path is not writable, it falls back to a
sibling `*-hl.pdf`. **Comment** pins a sticky note whose text is the
current selection (`*-note.pdf` if the open file is not writable).

**Form** lists existing AcroForm fields (`Name=value`) and fills a text
box, checkbox (`true` / `yes` / `on` / `1`), or radio button (the value
must match that button's export value). A combo box or list box selects a
listed option by its displayed label. A value that is not in that list is
rejected, and a compressed choice field is left unchanged. Type `Name=value`
and press Fill. The write stays in the open file and the page reloads; if
that path is not writable, it falls back to a sibling `*-form.pdf`.
Signatures are shown but not filled.

## Text

Tree-sitter highlighting, F3 view / F4 edit, HEX/binary, encodings,
find/replace, save.

## Archives

The file viewer browses ZIP, 7z, TAR, TAR.GZ, TAR.XZ, and TAR.BZ2. Creating
a password-protected archive, volumes, or an SFX needs 7-Zip. The file
manager's archive menu is described in [file-manager.md](file-manager.md).

## Media (libmpv)

In-app playback when the DLL is bundled; otherwise system-player handoff.
Playlist, subs, SMTC, EQ (Flat, Bass, Treble, Vocal), ReplayGain (off,
track, album). Viewer volume is **not** the Audio
Player widget volume.

## HTML

WebView2 overlay when the Evergreen runtime is installed; otherwise source
preview (size-capped) + Open in system browser.

## Document editor (DOCX / `.orchid`)

Tier-1 OOXML editor: Preview/Source, tables, images, styles, comments,
headers/footers (including first-page and even pages), Find/Replace, print.

**Save** to `.docx` or native `.orchid` (toolbar / Ctrl+Shift+S). Linked
`.orchid` autosaves after a ~2s debounce. Encrypted documents prompt for a
passphrase and keep the identity for re-encrypt on save. Info strip shows
generation, sealed vs linked, and C2PA status when present.

This is not Microsoft Word. Native format details:
[ORCHID_FORMAT.md](../ORCHID_FORMAT.md).
