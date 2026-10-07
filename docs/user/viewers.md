# Viewers

Opening a file (F3 / double-click / drop) routes by magic bytes and
extension. `.orchid` wraps unwrap Raw (except DOCX envelopes, which stay
in the document editor). Chrome keeps the `.orchid` path; temps are deleted
on close. OOXML packages (ZIP) are classified via `[Content_Types].xml`
when present in the file head: Word → document editor; Excel (`.xlsx`,
`.xlsm`) → a sheet table (switch sheets, click a cell or use the arrow
keys; the bar shows that cell's address and a field for its value; the
first 400 rows and 32 columns). Enter or Save writes that cell back into
the workbook. A formula cell is left unchanged. A value edit recalculates
arithmetic, comparisons, cell references, `SUM`, `AVERAGE`, `MIN`, `MAX`,
`COUNT`, `IF`, `ROUND`, `ABS`, `INT`, text joined with `&` or `CONCAT`,
and `LEN`, `LEFT`, `RIGHT`, `MID`, `UPPER`, `LOWER`, `TRIM`,
`SUBSTITUTE`, `FIND`, `SEARCH`, `REPT`, `EXACT`, `AND`, `OR`, `NOT`,
`SQRT`, `POWER`, `MOD`, `SIGN`, `PRODUCT`, `QUOTIENT`, `PI`, `EVEN`,
`ODD`, `REPLACE`, `VALUE`, `T`, `N`, `ROUNDUP`, `ROUNDDOWN`,
`CEILING.MATH`, `FLOOR.MATH`, `MEDIAN`, `ISNUMBER`, `ISTEXT`, `GCD`, `LCM`, `LN`, `LOG10`, `LOG`, `EXP`, `FACT`, `SIN`, `COS`, `TAN`, `RADIANS`, `DEGREES`, `ASIN`, `ACOS`, `ATAN`, `ATAN2`, `SINH`, `COSH`, `TANH`, `COMBIN`, `PERMUT`, `PERMUTATIONA`, `LARGE`, `SMALL`, `TRUNC`, `ISEVEN`, `ISODD`, `CODE`, `CHAR`, `COUNTA`, `COUNTBLANK`, `IFERROR`, `CLEAN`, `PROPER`, `CHOOSE`, `SWITCH`, `XOR`, `TEXTJOIN`, `IFS`, `BITAND`, `BITOR`, `BITXOR`, `CEILING`, `FLOOR`, `BITLSHIFT`, `BITRSHIFT`, `MROUND`, `SUMIF`, `COUNTIF`, `AVERAGEIF`, `SUMPRODUCT`, `MINIFS`, `MAXIFS`, `SUMIFS`, `AVERAGEIFS`, `SUMSQ`, `STDEV`, `STDEV.S`, `STDEVP`, `STDEV.P`, `VAR`, `VAR.S`, `VARP`, `VAR.P`, `AVEDEV`, `DEVSQ`, `GEOMEAN`, `HARMEAN`, `COUNTIFS`, `SLOPE`, `INTERCEPT`, `CORREL`, `PEARSON`, `RSQ`, `FORECAST`, `FORECAST.LINEAR`, `STEYX`, `COVARIANCE.P`, `COVAR`, `COVARIANCE.S`, `RANK`, `RANK.EQ`, `RANK.AVG`, `PERCENTILE`, `PERCENTILE.INC`, `PERCENTILE.EXC`, `QUARTILE`, `QUARTILE.INC`, `QUARTILE.EXC`, `MODE`, `MODE.SNGL`, `PERCENTRANK`, `PERCENTRANK.INC`, `PERCENTRANK.EXC`, `STANDARDIZE`, `SKEW`, `SKEW.P`, `KURT`, `TRIMMEAN`, `FISHER`, `FISHERINV`, `SQRTPI`, and `COMBINA` on that same sheet.
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
empty text. A missing cell inside a range is empty text, and a
shared-string cell is empty. A missing cell named on its own leaves the
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
and must be from -10 through 10. `CEILING.MATH` and `FLOOR.MATH` take one
number and move to an integer toward +infinity or -infinity. A
significance argument leaves the stored value. `CEILING` and `FLOOR` take
a significance. The signs must match. A zero significance makes `CEILING`
write 0 and makes `FLOOR` leave the stored value. `CEILING` moves away
from zero and `FLOOR` moves toward zero, to a multiple of that
significance. A magnitude at or above 1e15, or a call with one argument,
leaves the stored value. `MEDIAN` uses the same
numeric arguments as `SUM`, including a cell range. An even count averages
the two middle numbers. An empty call leaves the stored value. `ISNUMBER`
and `ISTEXT` write 1 or 0. A missing cell leaves the stored value, and a
shared-string cell is not classified. `GCD` and `LCM` use those same
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
value. `COUNTA` counts stored numbers and non-empty inline text.
`COUNTBLANK` counts the rest, including an empty inline string. A missing
cell inside a range counts as blank. A missing cell named on its own
leaves the stored value. A shared-string cell counts as blank. An empty
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
shared-string cells are skipped. No match writes 0. Other text, a third
argument, or a call that does not start with a range leaves the stored
value. `COUNTIF` uses that same range and criterion and writes how many
stored numbers match. No match writes 0. `AVERAGEIF` uses that same
range and criterion and writes the average. No match leaves the stored
value. `SUMPRODUCT` multiplies equal-sized cell ranges and adds the
products. One range is a sum. A blank cell, a text cell, or a shared-string
cell counts as 0. Up to 8 ranges are read. A different size, a ninth
range, or an argument that is not a range leaves the stored value.
`MINIFS` and `MAXIFS` take a value range, one criteria range of the same
size, and one criterion of the same kind. Only stored numbers are
considered. No match writes 0. A different size, other text, or a second
criterion leaves the stored value. `SUMIFS` adds those same
matching numbers. No match writes 0. A different size, other text, or a
second criterion leaves the stored value. `AVERAGEIFS` writes the
average of those same matching numbers. No match leaves the stored
value. A different size, other text, or a second criterion leaves the
stored value. `SUMSQ` adds the squares of the numbers `SUM` would
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
with one criterion, the same way `COUNTIF` does. No match writes 0.
Other text, a second criterion, or a call that does not start with a
range leaves the stored value. `SLOPE` takes a y range and an x range of
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
or a pair whose formula reaches 1,000,000 leaves the stored value. `IF` stays
numeric. Shared-string cells and formulas on another sheet are left as
stored. Other formulas keep their stored value. Drawings are copied through. A blank cell that the
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

## Media (libmpv)

In-app playback when the DLL is bundled; otherwise system-player handoff.
Playlist, subs, SMTC, EQ, ReplayGain. Viewer volume is **not** the Audio
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
