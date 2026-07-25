# DOCX fixtures (manual)

These fixtures must be real `.docx` files created in **Microsoft Word** or
**LibreOffice Writer**. Agents must not invent binary packages — place the
files here before running the round-trip integration tests in M2.9 / M7.5 / M7.6.

Expected files:

| File | Contents |
|------|----------|
| `basic_formatting.docx` | Bold, italic, underline, coloured text, custom font |
| `alignment.docx` | Left / center / right / justify paragraphs |
| `lists.docx` | Bulleted and numbered lists |
| `table_simple.docx` | 3×3 table without merged cells |
| `inline_image.docx` | One inline PNG image |
| `page_setup.docx` | Non-default page size / margins |

Synthetic in-memory ZIP fixtures cover unit tests without these files.
