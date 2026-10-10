mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn zip_bytes(files: &[(&str, &str)]) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut cursor);
            let opts = SimpleFileOptions::default();
            for (name, body) in files {
                zip.start_file(*name, opts).unwrap();
                zip.write_all(body.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        cursor.into_inner()
    }

    #[test]
    fn workbook_table_keeps_sheet_name_and_cells() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/sharedStrings.xml",
                r#"<sst><si><t>Orchid</t></si></sst>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1"><v>42</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let preview = render_office(&bytes, false).unwrap();
        let OfficePreview::Sheets(book) = preview else {
            panic!("workbook should be a sheet table");
        };
        assert_eq!(book.sheets.len(), 1, "{}", book.info);
        assert_eq!(book.sheets[0].name, "Budgets");
        let row = &book.sheets[0].rows[0];
        assert_eq!(row[0].text, "Orchid");
        assert_eq!(row[0].address, "A1");
        assert_eq!(row[1].text, "42");
        assert_eq!(row[1].address, "B1");
        assert!(book.info.contains("1 sheets"), "{}", book.info);
        assert!(!row[0].highlight);
    }

    #[test]
    fn sheet_preview_hides_filtered_rows() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="D1" t="inlineStr"><is><t>Item</t></is></c></row><row r="2" hidden="1"><c r="A2"><v>9</v></c></row><row r="3"><c r="A3"><v>3</v></c><c r="D3" t="inlineStr"><is><t>apple</t></is></c></row><row r="4"><c r="A4"><v>4</v></c><c r="D4" t="inlineStr"><is><t>pear</t></is></c></row></sheetData><autoFilter ref="D1:D4"><filterColumn colId="0"><filters><filter val="apple"/></filters></filterColumn></autoFilter></worksheet>"#,
            ),
        ]);
        let preview = render_office(&bytes, false).unwrap();
        let OfficePreview::Sheets(book) = preview else {
            panic!("workbook should be a sheet table");
        };
        let addresses: Vec<&str> = book.sheets[0]
            .rows
            .iter()
            .flatten()
            .map(|cell| cell.address.as_str())
            .collect();
        assert!(addresses.contains(&"A1"), "{addresses:?}");
        assert!(addresses.contains(&"D1"), "{addresses:?}");
        assert!(
            !addresses.contains(&"A2"),
            "a hidden row is omitted {addresses:?}"
        );
        assert!(addresses.contains(&"A3"), "{addresses:?}");
        assert!(addresses.contains(&"D3"), "{addresses:?}");
        assert!(
            !addresses.contains(&"A4"),
            "pear does not match the filter {addresses:?}"
        );
        assert!(!addresses.contains(&"D4"), "{addresses:?}");
    }

    #[test]
    fn sheet_preview_frozen_panes() {
        let sheet = |pane: &str| {
            format!(
                "<worksheet>{pane}<sheetData><row r=\"1\"><c r=\"A1\"><v>1</v></c><c r=\"B1\"><v>2</v></c></row><row r=\"2\"><c r=\"A2\"><v>3</v></c></row><row r=\"3\"><c r=\"A3\"><v>4</v></c></row></sheetData></worksheet>"
            )
        };
        let frozen = sheet(
            r#"<sheetViews><sheetView><pane xSplit="1" ySplit="2" state="frozen"/></sheetView></sheetViews>"#,
        );
        let split = sheet(
            r#"<sheetViews><sheetView><pane xSplit="1" ySplit="2" state="split"/></sheetView></sheetViews>"#,
        );
        let clamp = sheet(
            r#"<sheetViews><sheetView><pane xSplit="9" ySplit="20" state="frozenSplit"/></sheetView></sheetViews>"#,
        );
        let plain = sheet("");
        let rows_only = sheet(
            r#"<sheetViews><sheetView><pane ySplit="1" state="frozen"/></sheetView></sheetViews>"#,
        );
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Frozen" sheetId="1" r:id="rId1"/><sheet name="Split" sheetId="2" r:id="rId2"/><sheet name="Clamp" sheetId="3" r:id="rId3"/><sheet name="Plain" sheetId="4" r:id="rId4"/><sheet name="Rows" sheetId="5" r:id="rId5"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Target="worksheets/sheet2.xml"/><Relationship Id="rId3" Target="worksheets/sheet3.xml"/><Relationship Id="rId4" Target="worksheets/sheet4.xml"/><Relationship Id="rId5" Target="worksheets/sheet5.xml"/></Relationships>"#,
            ),
            ("xl/worksheets/sheet1.xml", frozen.as_str()),
            ("xl/worksheets/sheet2.xml", split.as_str()),
            ("xl/worksheets/sheet3.xml", clamp.as_str()),
            ("xl/worksheets/sheet4.xml", plain.as_str()),
            ("xl/worksheets/sheet5.xml", rows_only.as_str()),
        ]);
        let preview = render_office(&bytes, false).unwrap();
        let OfficePreview::Sheets(book) = preview else {
            panic!("workbook should be a sheet table");
        };
        assert_eq!(book.sheets[0].name, "Frozen");
        assert_eq!(
            (book.sheets[0].freeze_rows, book.sheets[0].freeze_cols),
            (2, 1)
        );
        assert_eq!(
            (book.sheets[1].freeze_rows, book.sheets[1].freeze_cols),
            (0, 0)
        );
        assert_eq!(
            (book.sheets[2].freeze_rows, book.sheets[2].freeze_cols),
            (8, 4)
        );
        assert_eq!(
            (book.sheets[3].freeze_rows, book.sheets[3].freeze_cols),
            (0, 0)
        );
        assert_eq!(
            (book.sheets[4].freeze_rows, book.sheets[4].freeze_cols),
            (1, 0)
        );
    }

    #[test]
    fn sheet_preview_color_scale() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>0</v></c><c r="C1"><v>25</v></c><c r="D1"><v>7</v></c><c r="E1"><v>3</v></c><c r="F1"><v>9</v></c><c r="G1"><v>4</v></c><c r="I1"><v>5</v></c><c r="J1"><v>6</v></c><c r="K1"><v>5</v></c></row><row r="2"><c r="A2"><v>50</v></c><c r="B2"><v>50</v></c></row><row r="3"><c r="A3"><v>100</v></c><c r="B3"><v>100</v></c></row></sheetData><conditionalFormatting sqref="I1:I33"><cfRule type="colorScale"><colorScale><cfvo type="min"/><cfvo type="max"/><color rgb="FFFF0000"/><color rgb="FF00FF00"/></colorScale></cfRule></conditionalFormatting><conditionalFormatting sqref="E1"><cfRule type="colorScale"><colorScale><cfvo type="formula" val="0"/><cfvo type="max"/><color rgb="FFFF0000"/><color rgb="FF00FF00"/></colorScale></cfRule></conditionalFormatting><conditionalFormatting sqref="J1"><cfRule type="colorScale"><colorScale><cfvo type="min"/><cfvo type="max"/><color theme="4"/><color rgb="FF00FF00"/></colorScale></cfRule></conditionalFormatting><conditionalFormatting sqref="K1"><cfRule type="colorScale"><colorScale><cfvo type="num" val="10"/><cfvo type="num" val="0"/><color rgb="FFFF0000"/><color rgb="FF00FF00"/></colorScale></cfRule></conditionalFormatting><conditionalFormatting sqref="A1:A3"><cfRule type="cellIs" operator="greaterThan"><formula>-1</formula></cfRule><cfRule type="colorScale"><colorScale><cfvo type="min"/><cfvo type="max"/><color rgb="FFFF0000"/><color rgb="FF00FF00"/></colorScale></cfRule></conditionalFormatting><conditionalFormatting sqref="B1:B3"><cfRule type="colorScale"><colorScale><cfvo type="min"/><cfvo type="percentile" val="50"/><cfvo type="max"/><color rgb="FFFF0000"/><color rgb="FFFFFF00"/><color rgb="FF00FF00"/></colorScale></cfRule></conditionalFormatting><conditionalFormatting sqref="C1"><cfRule type="colorScale"><colorScale><cfvo type="num" val="0"/><cfvo type="num" val="100"/><color rgb="FF0000"/><color rgb="0000FF"/></colorScale></cfRule></conditionalFormatting><conditionalFormatting sqref="D1"><cfRule type="colorScale"><colorScale><cfvo type="min"/><cfvo type="max"/><color rgb="FFFF0000"/><color rgb="FF00FF00"/></colorScale></cfRule></conditionalFormatting><conditionalFormatting sqref="F1"><cfRule type="colorScale"><colorScale><cfvo type="min"/><cfvo type="max"/><color rgb="FF0000FF"/><color rgb="FF0000FF"/></colorScale></cfRule></conditionalFormatting><conditionalFormatting sqref="G1"><cfRule type="dataBar"><dataBar><cfvo type="min"/><cfvo type="max"/></dataBar></cfRule></conditionalFormatting></worksheet>"#,
            ),
        ]);
        let preview = render_office(&bytes, false).unwrap();
        let OfficePreview::Sheets(book) = preview else {
            panic!("workbook should be a sheet table");
        };
        let cell = |address: &str| {
            book.sheets[0]
                .rows
                .iter()
                .flatten()
                .find(|cell| cell.address == address)
                .unwrap_or_else(|| panic!("missing {address}"))
        };
        let painted = |address: &str, red: u8, green: u8, blue: u8| {
            let cell = cell(address);
            assert!(cell.has_fill, "{address} should be painted");
            assert_eq!(
                (cell.fill_r, cell.fill_g, cell.fill_b),
                (red, green, blue),
                "{address}"
            );
        };
        painted("A1", 255, 0, 0);
        painted("A2", 128, 128, 0);
        painted("A3", 0, 255, 0);
        assert!(cell("A1").highlight, "a comparison rule still highlights");
        painted("B1", 255, 0, 0);
        painted("B2", 255, 255, 0);
        painted("B3", 0, 255, 0);
        painted("C1", 191, 0, 64);
        painted("D1", 255, 0, 0);
        for address in ["E1", "F1", "G1", "I1", "J1", "K1"] {
            assert!(!cell(address).has_fill, "{address} stays unpainted");
        }
    }

    #[test]
    fn sheet_preview_data_bar() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>8</v></c><c r="C1"><v>3</v></c><c r="D1"><v>25</v></c><c r="E1"><v>150</v></c><c r="F1"><v>-10</v></c><c r="G1"><v>4</v></c><c r="H1"><v>7</v></c><c r="I1"><v>5</v></c><c r="J1"><v>6</v></c><c r="K1"><v>-10</v></c><c r="L1"><v>-6</v></c><c r="M1"><v>-2</v></c><c r="N1" t="inlineStr"><is><t>x</t></is></c></row><row r="2"><c r="A2"><v>50</v></c></row><row r="3"><c r="A3"><v>100</v></c></row></sheetData><conditionalFormatting sqref="I1:I33"><cfRule type="dataBar"><dataBar><cfvo type="min"/><cfvo type="max"/><color rgb="FFFF0000"/></dataBar></cfRule></conditionalFormatting><conditionalFormatting sqref="H1"><cfRule type="dataBar"><dataBar><cfvo type="formula" val="0"/><cfvo type="max"/><color rgb="FFFF0000"/></dataBar></cfRule></conditionalFormatting><conditionalFormatting sqref="J1"><cfRule type="dataBar"><dataBar><cfvo type="min"/><cfvo type="max"/><color theme="4"/></dataBar></cfRule></conditionalFormatting><conditionalFormatting sqref="G1"><cfRule type="dataBar"><dataBar><cfvo type="num" val="10"/><cfvo type="num" val="0"/><color rgb="FFFF0000"/></dataBar></cfRule></conditionalFormatting><conditionalFormatting sqref="A1:A3"><cfRule type="dataBar"><dataBar><cfvo type="percentile" val="0"/><cfvo type="percentile" val="100"/><color rgb="FF0000FF"/></dataBar></cfRule><cfRule type="dataBar"><dataBar><cfvo type="min"/><cfvo type="max"/><color rgb="FFFF0000"/></dataBar></cfRule></conditionalFormatting><conditionalFormatting sqref="D1 E1 F1"><cfRule type="colorScale"><colorScale><cfvo type="min"/><cfvo type="max"/><color rgb="FFFF0000"/><color rgb="FF00FF00"/></colorScale></cfRule><cfRule type="dataBar"><dataBar><cfvo type="num" val="0"/><cfvo type="num" val="100"/><color rgb="FF00FF00"/></dataBar></cfRule></conditionalFormatting><conditionalFormatting sqref="K1:M1 N1"><cfRule type="dataBar"><dataBar><cfvo type="min"/><cfvo type="max"/></dataBar></cfRule></conditionalFormatting><conditionalFormatting sqref="B1"><cfRule type="dataBar"><dataBar><cfvo type="min"/><cfvo type="max"/><color rgb="FF112233"/></dataBar></cfRule></conditionalFormatting><conditionalFormatting sqref="C1"><cfRule type="iconSet"><iconSet><cfvo type="percent" val="0"/></iconSet></cfRule></conditionalFormatting></worksheet>"#,
            ),
        ]);
        let preview = render_office(&bytes, false).unwrap();
        let OfficePreview::Sheets(book) = preview else {
            panic!("workbook should be a sheet table");
        };
        let cell = |address: &str| {
            book.sheets[0]
                .rows
                .iter()
                .flatten()
                .find(|cell| cell.address == address)
                .unwrap_or_else(|| panic!("missing {address}"))
        };
        let barred = |address: &str, pct: u8, red: u8, green: u8, blue: u8| {
            let cell = cell(address);
            assert!(cell.has_bar, "{address} should have a bar");
            assert_eq!(
                (cell.bar_pct, cell.bar_r, cell.bar_g, cell.bar_b),
                (pct, red, green, blue),
                "{address}"
            );
        };
        barred("A1", 0, 0, 0, 255);
        barred("A2", 50, 0, 0, 255);
        barred("A3", 100, 0, 0, 255);
        barred("D1", 25, 0, 255, 0);
        barred("E1", 100, 0, 255, 0);
        barred("F1", 0, 0, 255, 0);
        assert!(cell("D1").has_fill, "a color scale still paints");
        barred("K1", 0, 0x63, 0x8E, 0xC6);
        barred("L1", 50, 0x63, 0x8E, 0xC6);
        barred("M1", 100, 0x63, 0x8E, 0xC6);
        for address in ["B1", "C1", "G1", "H1", "I1", "J1", "N1"] {
            assert!(!cell(address).has_bar, "{address} stays without a bar");
        }
    }

    #[test]
    fn sheet_preview_conditional_formatting() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>11</v></c><c r="B1"><v>5</v></c><c r="C1" t="inlineStr"><is><t>Pear</t></is></c><c r="D1" t="inlineStr"><is><t>Cat</t></is></c></row><row r="2"><c r="A2"><v>3</v></c></row><row r="3"><c r="A3"><v>100</v></c></row></sheetData><conditionalFormatting sqref="A1:A2"><cfRule type="cellIs" operator="greaterThan"><formula>10</formula></cfRule></conditionalFormatting><conditionalFormatting sqref="B1"><cfRule type="cellIs" operator="equal"><formula>5</formula></cfRule></conditionalFormatting><conditionalFormatting sqref="C1"><cfRule type="cellIs" operator="equal"><formula>&quot;pear&quot;</formula></cfRule></conditionalFormatting><conditionalFormatting sqref="D1"><cfRule type="containsText"><formula>NOT(ISERROR(SEARCH(&quot;c*&quot;,D1)))</formula></cfRule></conditionalFormatting><conditionalFormatting sqref="A1"><cfRule type="expression"><formula>A1&gt;0</formula></cfRule></conditionalFormatting><conditionalFormatting sqref="A3:A35"><cfRule type="cellIs" operator="greaterThan"><formula>1</formula></cfRule></conditionalFormatting></worksheet>"#,
            ),
        ]);
        let preview = render_office(&bytes, false).unwrap();
        let OfficePreview::Sheets(book) = preview else {
            panic!("workbook should be a sheet table");
        };
        let cell = |address: &str| {
            book.sheets[0]
                .rows
                .iter()
                .flatten()
                .find(|cell| cell.address == address)
                .unwrap_or_else(|| panic!("missing {address}"))
        };
        assert!(cell("A1").highlight, "11 is greater than 10");
        assert!(!cell("A2").highlight, "3 is not greater than 10");
        assert!(cell("B1").highlight);
        assert!(cell("C1").highlight);
        assert!(cell("D1").highlight);
        assert!(!cell("A3").highlight, "a range past 32 cells is ignored");
    }

    #[test]
    fn sheet_preview_expression_formatting() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>11</v></c><c r="C1"><v>4</v></c><c r="E1"><v>1</v></c><c r="F1"><v>1</v></c><c r="G1"><v>5</v></c><c r="H1"><v>5</v></c><c r="I1"><v>4</v></c><c r="J1"><v>5</v></c><c r="M1"><v>1</v></c></row><row r="2"><c r="B2"><v>1</v></c><c r="D2"><v>1</v></c><c r="E2"><v>9</v></c><c r="F2"><v>9</v></c></row><row r="3"><c r="B3"><v>0</v></c></row></sheetData><conditionalFormatting sqref="D2"><cfRule type="expression"><formula>SUM(D2)&gt;0</formula></cfRule></conditionalFormatting><conditionalFormatting sqref="M1"><cfRule type="expression"><formula>M1&gt;0+1</formula></cfRule></conditionalFormatting><conditionalFormatting sqref="B2:B3"><cfRule type="expression"><formula>=B2&gt;0</formula></cfRule></conditionalFormatting><conditionalFormatting sqref="C1"><cfRule type="expression"><formula>A1&gt;10</formula></cfRule></conditionalFormatting><conditionalFormatting sqref="E1:E2"><cfRule type="expression"><formula>E1&gt;5</formula></cfRule></conditionalFormatting><conditionalFormatting sqref="F1:F2"><cfRule type="expression"><formula>$F$1&gt;5</formula></cfRule></conditionalFormatting><conditionalFormatting sqref="G1"><cfRule type="expression"><formula>G1&gt;=5</formula></cfRule></conditionalFormatting><conditionalFormatting sqref="H1"><cfRule type="expression"><formula>H1&lt;&gt;5</formula></cfRule></conditionalFormatting><conditionalFormatting sqref="I1"><cfRule type="expression"><formula>I1&lt;=4</formula></cfRule></conditionalFormatting><conditionalFormatting sqref="J1"><cfRule type="expression"><formula>J1=5</formula></cfRule></conditionalFormatting></worksheet>"#,
            ),
        ]);
        let preview = render_office(&bytes, false).unwrap();
        let OfficePreview::Sheets(book) = preview else {
            panic!("workbook should be a sheet table");
        };
        let cell = |address: &str| {
            book.sheets[0]
                .rows
                .iter()
                .flatten()
                .find(|cell| cell.address == address)
                .unwrap_or_else(|| panic!("missing {address}"))
        };
        assert!(cell("B2").highlight, "1 is greater than 0");
        assert!(!cell("B3").highlight, "0 is not greater than 0");
        assert!(cell("C1").highlight, "the rule reads A1");
        assert!(!cell("D2").highlight, "a function is skipped");
        assert!(!cell("E1").highlight);
        assert!(cell("E2").highlight, "the address shifts down one row");
        assert!(!cell("F1").highlight);
        assert!(!cell("F2").highlight, "a locked address stays on F1");
        assert!(cell("G1").highlight);
        assert!(!cell("H1").highlight);
        assert!(cell("I1").highlight);
        assert!(cell("J1").highlight);
        assert!(
            !cell("M1").highlight,
            "arithmetic is not a plain comparison"
        );
    }

    #[test]
    fn sheet_preview_skipped_formula_rules_do_not_fill_the_cap() {
        let mut rules = String::new();
        for _ in 0..8 {
            rules.push_str(
                r#"<conditionalFormatting sqref="A1"><cfRule type="expression"><formula>SUM(A1)&gt;0</formula></cfRule></conditionalFormatting>"#,
            );
        }
        let sheet = format!(
            r#"<worksheet><sheetData><row r="1"><c r="A1"><v>3</v></c><c r="N1"><v>2</v></c></row></sheetData>{rules}<conditionalFormatting sqref="N1"><cfRule type="cellIs" operator="greaterThan"><formula>0</formula></cfRule></conditionalFormatting></worksheet>"#
        );
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            ("xl/worksheets/sheet1.xml", sheet.as_str()),
        ]);
        let preview = render_office(&bytes, false).unwrap();
        let OfficePreview::Sheets(book) = preview else {
            panic!("workbook should be a sheet table");
        };
        let cell = |address: &str| {
            book.sheets[0]
                .rows
                .iter()
                .flatten()
                .find(|cell| cell.address == address)
                .unwrap_or_else(|| panic!("missing {address}"))
        };
        assert!(!cell("A1").highlight, "a function rule does not count");
        assert!(cell("N1").highlight, "skipped rules do not fill the 8");
    }

    #[test]
    fn sheet_preview_merges_and_widths() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><cols><col min="1" max="1" width="10"/><col min="2" max="2" width="20"/><col min="4" max="4" width="100"/></cols><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Hi</t></is></c><c r="B1"><v>2</v></c><c r="C1"><v>3</v></c><c r="D1"><v>4</v></c></row><row r="2"><c r="A2"><v>5</v></c><c r="C2"><v>6</v></c></row><row r="3"><c r="A3"><v>7</v></c></row></sheetData><mergeCells><mergeCell ref="A1:B1"/><mergeCell ref="A2:A3"/><mergeCell ref="C2:K2"/></mergeCells></worksheet>"#,
            ),
        ]);
        let preview = render_office(&bytes, false).unwrap();
        let OfficePreview::Sheets(book) = preview else {
            panic!("workbook should be a sheet table");
        };
        let cell = |address: &str| {
            book.sheets[0]
                .rows
                .iter()
                .flatten()
                .find(|cell| cell.address == address)
                .unwrap_or_else(|| panic!("missing {address}"))
        };
        let origin = cell("A1");
        assert_eq!(origin.text, "Hi");
        assert_eq!(origin.span, 2);
        assert!(!origin.covered);
        assert_eq!(origin.width_px, 240);
        assert!(cell("B1").covered);
        assert_eq!(cell("B1").width_px, 0);
        assert_eq!(cell("A2").span, 1);
        assert_eq!(cell("A2").width_px, 80);
        assert!(cell("A3").covered);
        assert_eq!(cell("C1").width_px, 72);
        assert_eq!(
            cell("C2").span,
            1,
            "a merge wider than 8 columns is skipped"
        );
        assert!(!cell("C2").covered);
        assert_eq!(cell("D1").width_px, 320);
    }

    #[test]
    fn sheet_preview_legacy_comments() {
        let mut comments = String::from(
            r#"<comments><commentList><comment ref="A1" authorId="1"><text><t>Hello &amp; there</t></text></comment><comment ref="D1"><text><r><t>One</t></r><r><t> two</t></r></text></comment>"#,
        );
        for index in 1..=30 {
            comments.push_str(&format!(
                r#"<comment ref="Z{index}"><text><t>skip</t></text></comment>"#
            ));
        }
        comments.push_str(
            r#"<comment ref="B1"><text><t>Late</t></text></comment></commentList></comments>"#,
        );
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                r#"<Relationships><Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="../comments1.xml"/></Relationships>"#,
            ),
            ("xl/comments1.xml", &comments),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>2</v></c><c r="D1"><v>4</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let preview = render_office(&bytes, false).unwrap();
        let OfficePreview::Sheets(book) = preview else {
            panic!("workbook should be a sheet table");
        };
        let cell = |address: &str| {
            book.sheets[0]
                .rows
                .iter()
                .flatten()
                .find(|cell| cell.address == address)
                .unwrap_or_else(|| panic!("missing {address}"))
        };
        assert_eq!(cell("A1").note, "Hello & there");
        assert_eq!(cell("D1").note, "One two");
        assert!(cell("B1").note.is_empty(), "the 33rd comment is ignored");
    }

    #[test]
    fn sheet_preview_threaded_comments() {
        let long = "a".repeat(300);
        let mut threaded = format!(
            r#"<threadedComments><threadedComment ref="A1" personId="p1"><text>Later</text></threadedComment><threadedComment ref="B1" personId="p1"><text>One</text></threadedComment><threadedComment ref="B1" parentId="p1"><text>Two</text></threadedComment><threadedComment ref="C1"><text>{long}</text></threadedComment><threadedComment ref="E1"><text>Plain</text></threadedComment>"#
        );
        for index in 1..=27 {
            threaded.push_str(&format!(
                r#"<threadedComment ref="Z{index}"><text>skip</text></threadedComment>"#
            ));
        }
        threaded.push_str(
            r#"<threadedComment ref="D1"><text>Late</text></threadedComment></threadedComments>"#,
        );
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                r#"<Relationships><Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="../comments1.xml"/><Relationship Id="rId6" Type="http://schemas.microsoft.com/office/2017/10/relationships/threadedComment" Target="../threadedComments/threadedComment1.xml"/></Relationships>"#,
            ),
            (
                "xl/comments1.xml",
                r#"<comments><commentList><comment ref="A1"><text><t>Keep</t></text></comment></commentList></comments>"#,
            ),
            ("xl/threadedComments/threadedComment1.xml", &threaded),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>2</v></c><c r="C1"><v>3</v></c><c r="D1"><v>4</v></c><c r="E1"><v>5</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let preview = render_office(&bytes, false).unwrap();
        let OfficePreview::Sheets(book) = preview else {
            panic!("workbook should be a sheet table");
        };
        let cell = |address: &str| {
            book.sheets[0]
                .rows
                .iter()
                .flatten()
                .find(|cell| cell.address == address)
                .unwrap_or_else(|| panic!("missing {address}"))
        };
        assert_eq!(cell("A1").note, "Keep");
        assert_eq!(cell("B1").note, "One\nTwo");
        assert_eq!(cell("C1").note, "a".repeat(256));
        assert_eq!(cell("E1").note, "Plain");
        assert!(
            cell("D1").note.is_empty(),
            "the 33rd threaded comment is ignored"
        );
    }

    #[test]
    fn sheet_preview_number_formats() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/styles.xml",
                r#"<styleSheet><numFmts><numFmt numFmtId="164" formatCode="yyyy-mm-dd"/><numFmt numFmtId="165" formatCode="0.00"/></numFmts><cellXfs><xf numFmtId="9"/><xf numFmtId="3"/><xf numFmtId="14"/><xf numFmtId="164"/><xf numFmtId="165"/><xf numFmtId="10"/></cellXfs></styleSheet>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1" s="0"><v>0.25</v></c><c r="B1" s="1"><v>12345</v></c><c r="C1" s="2"><v>1</v></c><c r="D1" s="3"><v>61</v></c><c r="E1" s="4"><v>7</v></c><c r="F1" s="0" t="inlineStr"><is><t>ab</t></is></c><c r="G1" s="1"><v>12.5</v></c><c r="H1" s="5"><v>0.2</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let preview = render_office(&bytes, false).unwrap();
        let OfficePreview::Sheets(book) = preview else {
            panic!("workbook should be a sheet table");
        };
        let cell = |address: &str| {
            book.sheets[0]
                .rows
                .iter()
                .flatten()
                .find(|cell| cell.address == address)
                .unwrap_or_else(|| panic!("missing {address}"))
        };
        assert_eq!(cell("A1").text, "25%");
        assert_eq!(cell("B1").text, "12,345");
        assert_eq!(cell("C1").text, "1900-01-01");
        assert_eq!(cell("D1").text, "1900-03-01");
        assert_eq!(cell("E1").text, "7");
        assert_eq!(cell("F1").text, "ab");
        assert_eq!(cell("G1").text, "12.5");
        assert_eq!(cell("H1").text, "20%");
    }

    #[test]
    fn set_sheet_cell_rewrites_a_value_and_leaves_a_formula_and_drawing() {
        let drawing = "<drawing>keep-me</drawing>";
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/sharedStrings.xml",
                r#"<sst><si><t>Orchid</t></si></sst>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1"><v>42</v></c><c r="C1"><f>1+1</f><v>2</v></c></row></sheetData></worksheet>"#,
            ),
            ("xl/drawings/drawing1.xml", drawing),
        ]);
        let err = set_sheet_cell(&bytes, "Budgets", "C1", "9").unwrap_err();
        assert_eq!(err, "viewer-sheet-formula");
        let saved = set_sheet_cell(&bytes, "Budgets", "B1", "7").unwrap();
        let preview = render_office(&saved, false).unwrap();
        let OfficePreview::Sheets(book) = preview else {
            panic!("workbook should be a sheet table");
        };
        let row = &book.sheets[0].rows[0];
        assert_eq!(row[0].text, "Orchid");
        assert_eq!(row[1].text, "7");
        assert_eq!(row[2].text, "2");
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let kept = read_entry(&mut archive, "xl/drawings/drawing1.xml").unwrap();
        assert_eq!(kept, drawing);
        let renamed = set_sheet_cell(&bytes, "Budgets", "A1", "Lily & Rose").unwrap();
        let preview = render_office(&renamed, false).unwrap();
        let OfficePreview::Sheets(book) = preview else {
            panic!("workbook should be a sheet table");
        };
        let mut renamed_zip = ZipArchive::new(Cursor::new(renamed)).unwrap();
        let sheet = read_entry(&mut renamed_zip, "xl/worksheets/sheet1.xml").unwrap();
        let (rows, _) = parse_sheet(&sheet, &[], &[]);
        assert_eq!(rows[0][0].text, "Lily & Rose");
        assert_eq!(book.sheets[0].rows[0][0].text, "Lily & Rose");
        assert_eq!(book.sheets[0].rows[0][1].text, "42");
    }

    #[test]
    fn set_sheet_cell_recalculates_arithmetic_sum_and_average() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>2</v></c><c r="B1"><v>3</v></c><c r="C1"><f>A1+B1</f><v>0</v></c><c r="D1"><f>SUM(A1:A2)</f><v>0</v></c><c r="E1"><f>AVERAGE(A1,B1)</f><v>0</v></c><c r="F1"><f>ROMAN(A1)</f><v>9</v></c></row><row r="2"><c r="A2"><v>4</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let refused = set_sheet_cell(&bytes, "Budgets", "C1", "9").unwrap_err();
        assert_eq!(refused, "viewer-sheet-formula");
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "4").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<c r="C1"><f>A1+B1</f><v>7</v></c>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>SUM(A1:A2)</f><v>8</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>AVERAGE(A1,B1)</f><v>3.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="F1" t="inlineStr"><f>ROMAN(A1)</f><is><t>IV</t></is></c>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_recalculates_min_max_count_and_if() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>2</v></c><c r="B1"><v>8</v></c><c r="C1"><f>MIN(A1:B1)</f><v>0</v></c><c r="D1"><f>MAX(A1,B1)</f><v>0</v></c><c r="E1"><f>COUNT(A1:B1)</f><v>0</v></c><c r="F1"><f>IF(A1>5,B1,A1)</f><v>0</v></c><c r="G1"><f>IF(A1<>2,9,4)</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "B1", "3").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>MIN(A1:B1)</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MAX(A1,B1)</f><v>3</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COUNT(A1:B1)</f><v>2</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>IF(A1>5,B1,A1)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>IF(A1<>2,9,4)</f><v>4</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_rounds_and_joins_text() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1.26</v></c><c r="B1"><f>ROUND(A1,1)</f><v>0</v></c><c r="C1"><f>ABS(-4)</f><v>0</v></c><c r="D1"><f>INT(-1.2)</f><v>0</v></c><c r="E1" t="inlineStr"><is><t>x</t></is></c><c r="F1"><f>A1&amp;E1</f><v>0</v></c><c r="G1"><f>CONCAT("a","b")</f><v>0</v></c><c r="H1"><f>ROMAN(A1)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2.26").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>ROUND(A1,1)</f><v>2.3</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ABS(-4)</f><v>4</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>INT(-1.2)</f><v>-2</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<c r="F1" t="inlineStr"><f>A1&amp;E1</f><is><t>2.26x</t></is></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<c r="G1" t="inlineStr"><f>CONCAT("a","b")</f><is><t>ab</t></is></c>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="H1" t="inlineStr"><f>ROMAN(A1)</f><is><t>II</t></is></c>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_measures_and_slices_text() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>LEN("Orchid")</f><v>0</v></c><c r="C1"><f>LEFT("Orchid",2)</f><v>0</v></c><c r="D1"><f>RIGHT("Orchid",3)</f><v>0</v></c><c r="E1"><f>MID("Orchid",2,3)</f><v>0</v></c><c r="F1"><f>UPPER("ab")</f><v>0</v></c><c r="G1"><f>LOWER("AB")</f><v>0</v></c><c r="H1"><f>LEFT("ab",-1)</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>LEN("Orchid")</f><v>6</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>LEFT("Orchid",2)</f><is><t>Or</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RIGHT("Orchid",3)</f><is><t>hid</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MID("Orchid",2,3)</f><is><t>rch</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>UPPER("ab")</f><is><t>AB</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOWER("AB")</f><is><t>ab</t></is>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>LEFT("ab",-1)</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_trims_finds_and_repeats_text() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>TRIM("  a   b  ")</f><v>0</v></c><c r="C1"><f>SUBSTITUTE("ababa","a","X")</f><v>0</v></c><c r="D1"><f>SUBSTITUTE("ababa","a","X",2)</f><v>0</v></c><c r="E1"><f>FIND("ch","Orchid")</f><v>0</v></c><c r="F1"><f>SEARCH("CH","Orchid")</f><v>0</v></c><c r="G1"><f>FIND("CH","Orchid")</f><v>7</v></c><c r="H1"><f>REPT("ab",3)</f><v>0</v></c><c r="I1"><f>EXACT("Ab","Ab")</f><v>0</v></c><c r="J1"><f>EXACT("Ab","ab")</f><v>0</v></c><c r="K1"><f>REPT("a",-1)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>TRIM("  a   b  ")</f><is><t>a b</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUBSTITUTE("ababa","a","X")</f><is><t>XbXbX</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUBSTITUTE("ababa","a","X",2)</f><is><t>abXba</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FIND("ch","Orchid")</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SEARCH("CH","Orchid")</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FIND("CH","Orchid")</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>REPT("ab",3)</f><is><t>ababab</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>EXACT("Ab","Ab")</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>EXACT("Ab","ab")</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>REPT("a",-1)</f><v>9</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_search_wildcard() {
        let wide = "?".repeat(65);
        let hay = "a".repeat(257);
        let body = format!(
            r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>SEARCH("a*c","xxabyc")</f><v>0</v></c><c r="C1"><f>SEARCH("~*","a*b")</f><v>0</v></c><c r="D1"><f>SEARCH("?pple","Apple")</f><v>0</v></c><c r="E1"><f>FIND("a*","xa*b")</f><v>0</v></c><c r="F1"><f>SEARCH("z*","apple")</f><v>6</v></c><c r="G1"><f>SEARCH("*","abc")</f><v>0</v></c><c r="H1"><f>SEARCH("b*","abbc",3)</f><v>0</v></c><c r="I1"><f>SEARCH("{wide}","abc")</f><v>7</v></c><c r="J1"><f>SEARCH("a*","{hay}")</f><v>8</v></c></row></sheetData></worksheet>"#
        );
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            ("xl/worksheets/sheet1.xml", &body),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SEARCH("a*c","xxabyc")</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SEARCH("~*","a*b")</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SEARCH("?pple","Apple")</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FIND("a*","xa*b")</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SEARCH("z*","apple")</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SEARCH("*","abc")</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SEARCH("b*","abbc",3)</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(&format!(r#"<f>SEARCH("{wide}","abc")</f><v>7</v>"#)),
            "{sheet}"
        );
        assert!(
            sheet.contains(&format!(r#"<f>SEARCH("a*","{hay}")</f><v>8</v>"#)),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_and_or_not() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>AND(A1&gt;0,1)</f><v>0</v></c><c r="C1"><f>AND(A1&gt;5,1)</f><v>0</v></c><c r="D1"><f>OR(A1&gt;5,0)</f><v>0</v></c><c r="E1"><f>OR(0,A1)</f><v>0</v></c><c r="F1"><f>NOT(0)</f><v>0</v></c><c r="G1"><f>NOT(A1)</f><v>0</v></c><c r="H1"><f>AND()</f><v>7</v></c><c r="I1"><f>NOT("a")</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>AND(A1&gt;0,1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>AND(A1&gt;5,1)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>OR(A1&gt;5,0)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>OR(0,A1)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>NOT(0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>NOT(A1)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>AND()</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>NOT("a")</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_logic_range() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>2</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><v>0</v></c></row><row r="4"><c r="A4" t="inlineStr"><is><t>x</t></is></c></row><row r="5"><c r="A5"><f>AND(A1:A2)</f><v>0</v></c><c r="B5"><f>OR(A1:A2)</f><v>0</v></c><c r="C5"><f>XOR(A1:A2)</f><v>0</v></c><c r="D5"><f>AND(A1:A4)</f><v>0</v></c><c r="E5"><f>OR(A3:A4)</f><v>6</v></c><c r="F5"><f>AND(A1:A257)</f><v>7</v></c><c r="G5"><f>XOR(A1:B1)</f><v>0</v></c><c r="H5"><f>NOT(A1:A2)</f><v>8</v></c><c r="I5"><f>AND(A1:A2,1)</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>AND(A1:A2)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>OR(A1:A2)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>XOR(A1:A2)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>AND(A1:A4)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>OR(A3:A4)</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>AND(A1:A257)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>XOR(A1:B1)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>NOT(A1:A2)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>AND(A1:A2,1)</f><v>0</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_subtotal() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="C1" t="inlineStr"><is><t>x</t></is></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><v>2</v></c></row><row r="3"><c r="A3"><f>SUBTOTAL(9,A1:A2)</f><v>0</v></c><c r="B3"><f>SUBTOTAL(9,A1:A3)</f><v>0</v></c><c r="C3"><f>SUBTOTAL(3,A1:A2,C1)</f><v>0</v></c><c r="D3"><f>SUBTOTAL(101,A1:A2)</f><v>8</v></c><c r="E3"><f>SUBTOTAL(1,A1:A2)</f><v>0</v></c><c r="F3"><f>SUBTOTAL(2,A1:A2)</f><v>0</v></c><c r="G3"><f>SUBTOTAL(4,A1:A2)</f><v>0</v></c><c r="H3"><f>SUBTOTAL(5,A1:A2)</f><v>0</v></c><c r="I3"><f>SUBTOTAL(6,A1:A2)</f><v>0</v></c><c r="J3"><f>SUBTOTAL(9,A4)</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SUBTOTAL(9,A1:A2)</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUBTOTAL(9,A1:A3)</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUBTOTAL(3,A1:A2,C1)</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUBTOTAL(101,A1:A2)</f><v>1.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUBTOTAL(1,A1:A2)</f><v>1.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUBTOTAL(2,A1:A2)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUBTOTAL(4,A1:A2)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUBTOTAL(5,A1:A2)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUBTOTAL(6,A1:A2)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUBTOTAL(9,A4)</f><v>0</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_skips_hidden_rows_in_subtotal() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>SUBTOTAL(9,A1:A3)</f><v>0</v></c><c r="C1"><f>SUBTOTAL(109,A1:A3)</f><v>0</v></c><c r="F6"><f>SUBTOTAL(109,E6:E7)</f><v>0</v></c><c r="G6"><f>SUBTOTAL(9,E6:E7)</f><v>0</v></c><c r="Z1"><v>0</v></c></row><row r="2" hidden="1"><c r="A2"><v>2</v></c></row><row r="3"><c r="A3"><v>4</v></c></row><row r="5"><c r="D5" t="inlineStr"><is><t>Item</t></is></c></row><row r="6"><c r="D6" t="inlineStr"><is><t>apple</t></is></c><c r="E6"><v>5</v></c></row><row r="7"><c r="D7" t="inlineStr"><is><t>pear</t></is></c><c r="E7"><v>9</v></c></row></sheetData><autoFilter ref="D5:E7"><filterColumn colId="0"><filters><filter val="apple"/></filters></filterColumn></autoFilter></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SUBTOTAL(9,A1:A3)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUBTOTAL(109,A1:A3)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUBTOTAL(109,E6:E7)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUBTOTAL(9,E6:E7)</f><v>14</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"hidden="1""#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_sqrt_power_and_mod() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>SQRT(9)</f><v>0</v></c><c r="C1"><f>POWER(2,3)</f><v>0</v></c><c r="D1"><f>MOD(-3,2)</f><v>0</v></c><c r="E1"><f>MOD(3,-2)</f><v>0</v></c><c r="F1"><f>SQRT(-1)</f><v>7</v></c><c r="G1"><f>POWER(-8,0.5)</f><v>8</v></c><c r="H1"><f>MOD(5,0)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>SQRT(9)</f><v>3</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>POWER(2,3)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MOD(-3,2)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MOD(3,-2)</f><v>-1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SQRT(-1)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>POWER(-8,0.5)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MOD(5,0)</f><v>9</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_sign_product_and_quotient() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>SIGN(-4)</f><v>0</v></c><c r="C1"><f>SIGN(0)</f><v>0</v></c><c r="D1"><f>PRODUCT(2,3,4)</f><v>0</v></c><c r="E1"><f>PRODUCT()</f><v>0</v></c><c r="F1"><f>QUOTIENT(5,2)</f><v>0</v></c><c r="G1"><f>QUOTIENT(-5,2)</f><v>0</v></c><c r="H1"><f>QUOTIENT(5,0)</f><v>7</v></c><c r="I1"><f>EVEN(2.1)</f><v>0</v></c><c r="J1"><f>EVEN(-2.1)</f><v>0</v></c><c r="K1"><f>ODD(0)</f><v>0</v></c><c r="L1"><f>PI()</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>SIGN(-4)</f><v>-1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SIGN(0)</f><v>0</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>PRODUCT(2,3,4)</f><v>24</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>PRODUCT()</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>QUOTIENT(5,2)</f><v>2</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>QUOTIENT(-5,2)</f><v>-2</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>QUOTIENT(5,0)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>EVEN(2.1)</f><v>4</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>EVEN(-2.1)</f><v>-4</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ODD(0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>PI()</f><v>3.14159265</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_replaces_a_span() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>REPLACE("Orchid",3,2,"XY")</f><v>0</v></c><c r="C1"><f>REPLACE("ab",3,1,"X")</f><v>0</v></c><c r="D1"><f>REPLACE("abcd",2,10,"Z")</f><v>0</v></c><c r="E1"><f>REPLACE("ab",0,1,"X")</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>REPLACE("Orchid",3,2,"XY")</f><is><t>OrXYid</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>REPLACE("ab",3,1,"X")</f><is><t>abX</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>REPLACE("abcd",2,10,"Z")</f><is><t>aZ</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>REPLACE("ab",0,1,"X")</f><v>7</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_value_t_and_n() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>VALUE(" 2.5 ")</f><v>0</v></c><c r="C1"><f>VALUE("-3")</f><v>0</v></c><c r="D1"><f>VALUE("x")</f><v>7</v></c><c r="E1"><f>T("ab")</f><v>0</v></c><c r="F1"><f>T(4)</f><v>0</v></c><c r="G1"><f>N("ab")</f><v>0</v></c><c r="H1"><f>N(4)</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>VALUE(" 2.5 ")</f><v>2.5</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>VALUE("-3")</f><v>-3</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>VALUE("x")</f><v>7</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>T("ab")</f><is><t>ab</t></is>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>T(4)</f><is><t></t></is>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>N("ab")</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>N(4)</f><v>4</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_rounds_up_and_down() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>ROUNDUP(1.234,2)</f><v>0</v></c><c r="C1"><f>ROUNDDOWN(1.239,2)</f><v>0</v></c><c r="D1"><f>ROUNDUP(-1.234,2)</f><v>0</v></c><c r="E1"><f>ROUNDDOWN(-1.239,2)</f><v>0</v></c><c r="F1"><f>CEILING.MATH(-1.2)</f><v>0</v></c><c r="G1"><f>FLOOR.MATH(-1.2)</f><v>0</v></c><c r="H1"><f>ROUNDUP(1.2,20)</f><v>7</v></c><c r="I1"><f>CEILING.MATH(1.2,1)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>ROUNDUP(1.234,2)</f><v>1.24</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ROUNDDOWN(1.239,2)</f><v>1.23</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ROUNDUP(-1.234,2)</f><v>-1.24</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ROUNDDOWN(-1.239,2)</f><v>-1.23</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CEILING.MATH(-1.2)</f><v>-1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FLOOR.MATH(-1.2)</f><v>-2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ROUNDUP(1.2,20)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CEILING.MATH(1.2,1)</f><v>2</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_math_step() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>CEILING.MATH(-5.5,2)</f><v>0</v></c><c r="C1"><f>CEILING.MATH(-5.5,2,1)</f><v>0</v></c><c r="D1"><f>FLOOR.MATH(5.5,2)</f><v>0</v></c><c r="E1"><f>FLOOR.MATH(-5.5,1,1)</f><v>0</v></c><c r="F1"><f>CEILING.MATH(5,0)</f><v>0</v></c><c r="G1"><f>FLOOR.MATH(5,-2)</f><v>0</v></c><c r="H1"><f>CEILING.MATH(&quot;ab&quot;,1)</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>CEILING.MATH(-5.5,2)</f><v>-4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CEILING.MATH(-5.5,2,1)</f><v>-6</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FLOOR.MATH(5.5,2)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FLOOR.MATH(-5.5,1,1)</f><v>-5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CEILING.MATH(5,0)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FLOOR.MATH(5,-2)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CEILING.MATH(&quot;ab&quot;,1)</f><v>7</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_average_a() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><f>AVERAGEA(A1:C1)</f><v>0</v></c><c r="E1"><f>MINA(A1:C1)</f><v>0</v></c><c r="F1"><f>MAXA(A1:C1)</f><v>0</v></c><c r="G1"><f>STDEVA(A1:B1)</f><v>0</v></c><c r="H1"><f>VARA(A1:B1)</f><v>0</v></c><c r="I1"><f>AVERAGEA()</f><v>7</v></c><c r="J1"><f>AVERAGEA(&quot;ab&quot;)</f><v>0</v></c><c r="K1"><f>STDEVA(A1)</f><v>8</v></c><c r="L1"><f>MINA()</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>AVERAGEA(A1:C1)</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>MINA(A1:C1)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MAXA(A1:C1)</f><v>1</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>STDEVA(A1:B1)</f><v>0.70710678</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>VARA(A1:B1)</f><v>0.5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>AVERAGEA()</f><v>7</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>AVERAGEA(&quot;ab&quot;)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>STDEVA(A1)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MINA()</f><v>0</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_date_serial() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>DATE(1900,1,1)</f><v>0</v></c><c r="C1"><f>DATE(1900,2,28)</f><v>0</v></c><c r="D1"><f>DATE(1900,2,29)</f><v>0</v></c><c r="E1"><f>DATE(1900,3,1)</f><v>0</v></c><c r="F1"><f>DATE(108,1,2)</f><v>0</v></c><c r="G1"><f>DATE(2020,1,1)</f><v>0</v></c><c r="H1"><f>DATE(1900,1,0)</f><v>0</v></c><c r="I1"><f>YEAR(60)</f><v>0</v></c><c r="J1"><f>MONTH(60)</f><v>0</v></c><c r="K1"><f>DAY(60)</f><v>0</v></c><c r="L1"><f>YEAR(43831)</f><v>0</v></c><c r="M1"><f>MONTH(43831)</f><v>0</v></c><c r="N1"><f>DAY(43831)</f><v>0</v></c><c r="O1"><f>YEAR(0)</f><v>0</v></c><c r="P1"><f>MONTH(0)</f><v>0</v></c><c r="Q1"><f>DAY(0)</f><v>0</v></c><c r="R1"><f>DATE(-1,1,1)</f><v>9</v></c><c r="S1"><f>YEAR(-1)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>DATE(1900,1,1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DATE(1900,2,28)</f><v>59</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DATE(1900,2,29)</f><v>60</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DATE(1900,3,1)</f><v>61</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DATE(108,1,2)</f><v>39449</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DATE(2020,1,1)</f><v>43831</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DATE(1900,1,0)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>YEAR(60)</f><v>1900</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MONTH(60)</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>DAY(60)</f><v>29</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>YEAR(43831)</f><v>2020</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>MONTH(43831)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>DAY(43831)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>YEAR(0)</f><v>1900</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MONTH(0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>DAY(0)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>DATE(-1,1,1)</f><v>9</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>YEAR(-1)</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_base_conversion() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>DEC2BIN(5)</f><v>0</v></c><c r="C1"><f>DEC2BIN(5,4)</f><v>0</v></c><c r="D1"><f>DEC2BIN(-1)</f><v>0</v></c><c r="E1"><f>DEC2BIN(512)</f><v>9</v></c><c r="F1"><f>BIN2DEC(&quot;101&quot;)</f><v>0</v></c><c r="G1"><f>BIN2DEC(&quot;1111111111&quot;)</f><v>0</v></c><c r="H1"><f>DEC2HEX(255)</f><v>0</v></c><c r="I1"><f>DEC2HEX(-1)</f><v>0</v></c><c r="J1"><f>HEX2DEC(&quot;ff&quot;)</f><v>0</v></c><c r="K1"><f>HEX2DEC(&quot;FFFFFFFFFF&quot;)</f><v>0</v></c><c r="L1"><f>DEC2OCT(8)</f><v>0</v></c><c r="M1"><f>DEC2OCT(-1)</f><v>0</v></c><c r="N1"><f>OCT2DEC(&quot;10&quot;)</f><v>0</v></c><c r="O1"><f>OCT2DEC(&quot;7777777777&quot;)</f><v>0</v></c><c r="P1"><f>BASE(13,2,8)</f><v>0</v></c><c r="Q1"><f>BASE(255,16)</f><v>0</v></c><c r="R1"><f>DECIMAL(&quot;FF&quot;,16)</f><v>0</v></c><c r="S1"><f>DECIMAL(&quot;101&quot;,2)</f><v>0</v></c><c r="T1"><f>DEC2BIN(-1,4)</f><v>8</v></c><c r="U1"><f>BASE(1,2,0)</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<c r="B1" t="inlineStr"><f>DEC2BIN(5)</f><is><t>101</t></is></c>"#),
            "{sheet}"
        );
        assert!(
            sheet
                .contains(r#"<c r="C1" t="inlineStr"><f>DEC2BIN(5,4)</f><is><t>0101</t></is></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<c r="D1" t="inlineStr"><f>DEC2BIN(-1)</f><is><t>1111111111</t></is></c>"#
            ),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>DEC2BIN(512)</f><v>9</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>BIN2DEC(&quot;101&quot;)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BIN2DEC(&quot;1111111111&quot;)</f><v>-1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="H1" t="inlineStr"><f>DEC2HEX(255)</f><is><t>FF</t></is></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<c r="I1" t="inlineStr"><f>DEC2HEX(-1)</f><is><t>FFFFFFFFFF</t></is></c>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HEX2DEC(&quot;ff&quot;)</f><v>255</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HEX2DEC(&quot;FFFFFFFFFF&quot;)</f><v>-1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="L1" t="inlineStr"><f>DEC2OCT(8)</f><is><t>10</t></is></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<c r="M1" t="inlineStr"><f>DEC2OCT(-1)</f><is><t>7777777777</t></is></c>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>OCT2DEC(&quot;10&quot;)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>OCT2DEC(&quot;7777777777&quot;)</f><v>-1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<c r="P1" t="inlineStr"><f>BASE(13,2,8)</f><is><t>00001101</t></is></c>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="Q1" t="inlineStr"><f>BASE(255,16)</f><is><t>FF</t></is></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DECIMAL(&quot;FF&quot;,16)</f><v>255</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DECIMAL(&quot;101&quot;,2)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>DEC2BIN(-1,4)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>BASE(1,2,0)</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_bessel() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>BESSELJ(1,0)</f><v>0</v></c><c r="C1"><f>BESSELJ(1,1)</f><v>0</v></c><c r="D1"><f>BESSELI(1,0)</f><v>0</v></c><c r="E1"><f>BESSELI(1,1)</f><v>0</v></c><c r="F1"><f>BESSELJ(0,0)</f><v>0</v></c><c r="G1"><f>BESSELJ(0,1)</f><v>0</v></c><c r="H1"><f>BESSELJ(1,1.9)</f><v>0</v></c><c r="I1"><f>BESSELJ(1,-1)</f><v>9</v></c><c r="J1"><f>BESSELJ(40,0)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>BESSELJ(1,0)</f><v>0.76519769</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BESSELJ(1,1)</f><v>0.44005059</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BESSELI(1,0)</f><v>1.26606588</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BESSELI(1,1)</f><v>0.5651591</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>BESSELJ(0,0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>BESSELJ(0,1)</f><v>0</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>BESSELJ(1,1.9)</f><v>0.44005059</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>BESSELJ(1,-1)</f><v>9</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>BESSELJ(40,0)</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_mdeterm() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>2</v></c><c r="C1"><v>3</v></c><c r="D1"><f>MDETERM(A1:C3)</f><v>0</v></c><c r="E1"><v>1</v></c><c r="F1"><v>2</v></c><c r="G1"><f>MDETERM(E1:F2)</f><v>0</v></c><c r="H1"><v>5</v></c><c r="I1"><f>MDETERM(H1:H1)</f><v>0</v></c><c r="J1"><v>1</v></c><c r="L1"><f>MDETERM(J1:K2)</f><v>0</v></c><c r="M1" t="inlineStr"><is><t>xy</t></is></c><c r="N1"><v>1</v></c><c r="O1"><f>MDETERM(M1:N2)</f><v>9</v></c><c r="P1"><f>MDETERM(A1:B3)</f><v>8</v></c><c r="Q1"><v>1</v></c><c r="R1"><v>2</v></c><c r="S1"><f>MDETERM(Q1:R2)</f><v>0</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><v>0</v></c><c r="B2"><v>1</v></c><c r="C2"><v>4</v></c><c r="E2"><v>3</v></c><c r="F2"><v>4</v></c><c r="K2"><v>1</v></c><c r="M2"><v>0</v></c><c r="N2"><v>1</v></c><c r="Q2"><v>2</v></c><c r="R2"><v>4</v></c></row><row r="3"><c r="A3"><v>5</v></c><c r="B3"><v>6</v></c><c r="C3"><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>MDETERM(A1:C3)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MDETERM(E1:F2)</f><v>-2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MDETERM(H1:H1)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MDETERM(J1:K2)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MDETERM(M1:N2)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MDETERM(A1:B3)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MDETERM(Q1:R2)</f><v>0</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_lookup() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>10</v></c><c r="B1"><v>20</v></c><c r="C1"><v>30</v></c><c r="E1"><v>1</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><v>40</v></c><c r="B2"><v>50</v></c><c r="C2"><v>60</v></c></row><row r="3"><c r="A3" t="inlineStr"><is><t>Cat</t></is></c></row><row r="4"><c r="A4"><f>INDEX(A1:C2,2,3)</f><v>0</v></c><c r="B4"><f>INDEX(A1:A2,2)</f><v>0</v></c><c r="C4"><f>INDEX(A1:C2,1)</f><v>8</v></c><c r="D4"><f>INDEX(A1:C2,2,0)</f><v>7</v></c><c r="E4"><f>INDEX(A3:A3,1)</f><v>0</v></c><c r="F4"><f>MATCH(40,A1:A2,0)</f><v>0</v></c><c r="G4"><f>MATCH(20,A1:C1,0)</f><v>0</v></c><c r="H4"><f>MATCH(&quot;cat&quot;,A3:A3,0)</f><v>0</v></c><c r="I4"><f>MATCH(40,A1:A2,1)</f><v>6</v></c><c r="J4"><f>MATCH(99,A1:A2,0)</f><v>5</v></c><c r="K4"><f>VLOOKUP(40,A1:C2,3,0)</f><v>0</v></c><c r="L4"><f>VLOOKUP(10,A1:C2,2,0)</f><v>0</v></c><c r="M4"><f>VLOOKUP(40,A1:C2,3)</f><v>4</v></c><c r="N4"><f>HLOOKUP(20,A1:C2,2,0)</f><v>0</v></c><c r="O4"><f>HLOOKUP(30,A1:C2,2,1)</f><v>3</v></c><c r="P4"><f>INDEX(D1:D1,1)</f><v>0</v></c><c r="Q4"><f>MATCH(&quot;1&quot;,E1:E1,0)</f><v>2</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>INDEX(A1:C2,2,3)</f><v>60</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>INDEX(A1:A2,2)</f><v>40</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>INDEX(A1:C2,1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>INDEX(A1:C2,2,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<c r="E4" t="inlineStr"><f>INDEX(A3:A3,1)</f><is><t>Cat</t></is></c>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MATCH(40,A1:A2,0)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MATCH(20,A1:C1,0)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MATCH(&quot;cat&quot;,A3:A3,0)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MATCH(40,A1:A2,1)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MATCH(99,A1:A2,0)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>VLOOKUP(40,A1:C2,3,0)</f><v>60</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>VLOOKUP(10,A1:C2,2,0)</f><v>20</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>VLOOKUP(40,A1:C2,3)</f><v>60</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HLOOKUP(20,A1:C2,2,0)</f><v>50</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HLOOKUP(30,A1:C2,2,1)</f><v>60</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>INDEX(D1:D1,1)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MATCH(&quot;1&quot;,E1:E1,0)</f><v>2</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_lookup_walk() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>10</v></c><c r="B1"><f>MATCH(15,A1:A2,1)</f><v>0</v></c><c r="C1"><f>MATCH(5,A1:A2,1)</f><v>8</v></c><c r="D1"><f>MATCH(30,E1:E2,-1)</f><v>0</v></c><c r="E1"><v>40</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><v>40</v></c><c r="E2"><v>10</v></c></row><row r="3"><c r="A3" t="inlineStr"><is><t>Cat</t></is></c><c r="B3"><f>MATCH(&quot;c*&quot;,A3:A3,0)</f><v>0</v></c><c r="C3"><f>MATCH(&quot;c?&quot;,A3:A3,0)</f><v>7</v></c><c r="D3"><f>MATCH(&quot;c~*&quot;,A3:A3,0)</f><v>6</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>MATCH(15,A1:A2,1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MATCH(5,A1:A2,1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MATCH(30,E1:E2,-1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MATCH(&quot;c*&quot;,A3:A3,0)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MATCH(&quot;c?&quot;,A3:A3,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MATCH(&quot;c~*&quot;,A3:A3,0)</f><v>6</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_beta_dist() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>BETA.DIST(0.5,2,3,1)</f><v>0</v></c><c r="C1"><f>BETA.DIST(0.5,2,3,0)</f><v>0</v></c><c r="D1"><f>BETA.DIST(0.3,5,2,1)</f><v>0</v></c><c r="E1"><f>BETA.DIST(0.2,0.5,0.5,1)</f><v>0</v></c><c r="F1"><f>BETA.DIST(0.2,1,1,1)</f><v>0</v></c><c r="G1"><f>BETA.DIST(2,1,1,1)</f><v>9</v></c><c r="H1"><f>BETA.DIST(0.5,2,3)</f><v>8</v></c><c r="I1"><f>BETA.DIST(0.5,0,1,1)</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>BETA.DIST(0.5,2,3,1)</f><v>0.6875</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BETA.DIST(0.5,2,3,0)</f><v>1.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BETA.DIST(0.3,5,2,1)</f><v>0.010935</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BETA.DIST(0.2,0.5,0.5,1)</f><v>0.29516724</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BETA.DIST(0.2,1,1,1)</f><v>0.2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BETA.DIST(2,1,1,1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BETA.DIST(0.5,2,3)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BETA.DIST(0.5,0,1,1)</f><v>7</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_t_dist() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>T.DIST(1,1,1)</f><v>0</v></c><c r="C1"><f>T.DIST(0,1,0)</f><v>0</v></c><c r="D1"><f>T.DIST(2,5,1)</f><v>0</v></c><c r="E1"><f>T.DIST(2,5,0)</f><v>0</v></c><c r="F1"><f>T.DIST.RT(2,5)</f><v>0</v></c><c r="G1"><f>T.DIST.2T(2,5)</f><v>0</v></c><c r="H1"><f>TDIST(2,5,1)</f><v>0</v></c><c r="I1"><f>TDIST(2,5,2)</f><v>0</v></c><c r="J1"><f>T.DIST.2T(-1,5)</f><v>9</v></c><c r="K1"><f>TDIST(1,5,3)</f><v>8</v></c><c r="L1"><f>T.DIST(1,0,1)</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>T.DIST(1,1,1)</f><v>0.75</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>T.DIST(0,1,0)</f><v>0.31830989</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>T.DIST(2,5,1)</f><v>0.94903026</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>T.DIST(2,5,0)</f><v>0.06509031</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>T.DIST.RT(2,5)</f><v>0.05096974</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>T.DIST.2T(2,5)</f><v>0.10193948</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TDIST(2,5,1)</f><v>0.05096974</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TDIST(2,5,2)</f><v>0.10193948</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>T.DIST.2T(-1,5)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>TDIST(1,5,3)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>T.DIST(1,0,1)</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_f_dist() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>F.DIST(2,5,10,1)</f><v>0</v></c><c r="C1"><f>F.DIST(2,5,10,0)</f><v>0</v></c><c r="D1"><f>F.DIST.RT(2,5,10)</f><v>0</v></c><c r="E1"><f>FDIST(2,5,10)</f><v>0</v></c><c r="F1"><f>F.DIST(-1,5,10,1)</f><v>9</v></c><c r="G1"><f>F.DIST(2,0,10,1)</f><v>8</v></c><c r="H1"><f>FDIST(2,5)</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>F.DIST(2,5,10,1)</f><v>0.83580505</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>F.DIST(2,5,10,0)</f><v>0.16200574</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>F.DIST.RT(2,5,10)</f><v>0.16419495</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FDIST(2,5,10)</f><v>0.16419495</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>F.DIST(-1,5,10,1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>F.DIST(2,0,10,1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>FDIST(2,5)</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_t_test() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>2</v></c><c r="C1"><f>T.TEST(A1:A3,B1:B3,2,1)</f><v>0</v></c><c r="D1"><f>T.TEST(A1:A3,B1:B3,1,1)</f><v>0</v></c><c r="E1"><f>TTEST(A1:A3,B1:B3,2,2)</f><v>0</v></c><c r="F1"><f>T.TEST(A1:A3,B1:B3,2,3)</f><v>0</v></c><c r="G1"><f>F.TEST(A1:A3,B1:B3)</f><v>0</v></c><c r="H1"><f>CONFIDENCE.T(0.05,1,10)</f><v>0</v></c><c r="I1"><f>T.TEST(A1:A3,B1:B3,2,4)</f><v>9</v></c><c r="J1"><f>CONFIDENCE.T(0,1,10)</f><v>8</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><v>2</v></c><c r="B2"><v>3</v></c></row><row r="3"><c r="A3"><v>3</v></c><c r="B3"><v>5</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>T.TEST(A1:A3,B1:B3,2,1)</f><v>0.05719096</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>T.TEST(A1:A3,B1:B3,1,1)</f><v>0.02859548</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TTEST(A1:A3,B1:B3,2,2)</f><v>0.27457663</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>T.TEST(A1:A3,B1:B3,2,3)</f><v>0.28462718</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>F.TEST(A1:A3,B1:B3)</f><v>0.6</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONFIDENCE.T(0.05,1,10)</f><v>0.71535691</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>T.TEST(A1:A3,B1:B3,2,4)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONFIDENCE.T(0,1,10)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_annuity() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>-100</v></c><c r="B1"><f>PMT(0.01,10,-1000)</f><v>0</v></c><c r="C1"><f>PMT(0,10,-1000)</f><v>0</v></c><c r="D1"><f>PMT(0.01,10,-1000,0,1)</f><v>0</v></c><c r="E1"><f>FV(0.01,10,-100,-1000)</f><v>0</v></c><c r="F1"><f>PV(0.01,10,-100)</f><v>0</v></c><c r="G1"><f>NPER(0.01,-100,1000)</f><v>0</v></c><c r="H1"><f>NPER(0,-100,1000)</f><v>0</v></c><c r="I1"><f>RATE(12,-100,1000)</f><v>0</v></c><c r="J1"><f>NPV(0.1,100,200)</f><v>0</v></c><c r="K1"><f>IRR(A1:A3)</f><v>0</v></c><c r="L1"><f>RATE(1,1,1)</f><v>9</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><v>60</v></c></row><row r="3"><c r="A3"><v>60</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>PMT(0.01,10,-1000)</f><v>105.58207655</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PMT(0,10,-1000)</f><v>100</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PMT(0.01,10,-1000,0,1)</f><v>104.53670946</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FV(0.01,10,-100,-1000)</f><v>2150.84337952</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PV(0.01,10,-100)</f><v>947.13045307</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NPER(0.01,-100,1000)</f><v>10.58864446</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NPER(0,-100,1000)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RATE(12,-100,1000)</f><v>0.02922854</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NPV(0.1,100,200)</f><v>256.19834711</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>IRR(A1:A3)</f><v>0.13066239</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>RATE(1,1,1)</f><v>9</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_calendar() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>WEEKDAY(1)</f><v>0</v></c><c r="C1"><f>WEEKDAY(1,2)</f><v>0</v></c><c r="D1"><f>WEEKDAY(1,3)</f><v>0</v></c><c r="E1"><f>WEEKDAY(61,1)</f><v>0</v></c><c r="F1"><f>WEEKDAY(1,11)</f><v>9</v></c><c r="G1"><f>EDATE(43861,1)</f><v>0</v></c><c r="H1"><f>EOMONTH(43831,0)</f><v>0</v></c><c r="I1"><f>EOMONTH(43831,1)</f><v>0</v></c><c r="J1"><f>NETWORKDAYS(1,7)</f><v>0</v></c><c r="K1"><f>NETWORKDAYS(1,7,B2:B2)</f><v>0</v></c><c r="L1"><f>WORKDAY(1,5)</f><v>0</v></c><c r="M1"><f>WORKDAY(6,1)</f><v>0</v></c><c r="N1"><f>DATEDIF(43831,44256,&quot;Y&quot;)</f><v>0</v></c><c r="O1"><f>DATEDIF(43831,44256,&quot;M&quot;)</f><v>0</v></c><c r="P1"><f>DATEDIF(43831,44256,&quot;D&quot;)</f><v>0</v></c><c r="Q1"><f>DATEDIF(43831,44256,&quot;YM&quot;)</f><v>0</v></c><c r="R1"><f>DATEDIF(43831,44256,&quot;YD&quot;)</f><v>0</v></c><c r="S1"><f>DATEDIF(43831,44256,&quot;MD&quot;)</f><v>0</v></c></row><row r="2"><c r="B2"><v>2</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>WEEKDAY(1)</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>WEEKDAY(1,2)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>WEEKDAY(1,3)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>WEEKDAY(61,1)</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>WEEKDAY(1,11)</f><v>9</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>EDATE(43861,1)</f><v>43890</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>EOMONTH(43831,0)</f><v>43861</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>EOMONTH(43831,1)</f><v>43890</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NETWORKDAYS(1,7)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NETWORKDAYS(1,7,B2:B2)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>WORKDAY(1,5)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>WORKDAY(6,1)</f><v>8</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>DATEDIF(43831,44256,&quot;Y&quot;)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DATEDIF(43831,44256,&quot;M&quot;)</f><v>14</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DATEDIF(43831,44256,&quot;D&quot;)</f><v>425</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DATEDIF(43831,44256,&quot;YM&quot;)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DATEDIF(43831,44256,&quot;YD&quot;)</f><v>59</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DATEDIF(43831,44256,&quot;MD&quot;)</f><v>0</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_wildcards() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>cat</t></is></c><c r="B1"><v>10</v></c><c r="C1"><f>COUNTIF(A1:A3,&quot;c*&quot;)</f><v>0</v></c><c r="D1"><f>SUMIF(A1:A3,&quot;c*&quot;)</f><v>1</v></c><c r="E1"><f>AVERAGEIF(A1:A3,&quot;c*&quot;)</f><v>4</v></c><c r="F1"><f>SUMIFS(B1:B3,A1:A3,&quot;c*&quot;)</f><v>0</v></c><c r="G1"><f>COUNTIF(A4:A4,&quot;c~*&quot;)</f><v>5</v></c><c r="H1"><f>COUNTIF(A4:A4,&quot;c~**&quot;)</f><v>0</v></c><c r="I1"><f>COUNTIF(A1:A3,&quot;&lt;&gt;d*&quot;)</f><v>0</v></c><c r="J1"><f>COUNTIF(A1:A3,&quot;&gt;apple&quot;)</f><v>6</v></c><c r="K1"><f>SUMIF(A1:A3,&quot;ab&quot;)</f><v>7</v></c><c r="L1"><f>SUMIFS(B1:B3,A1:A3,&quot;c*&quot;,B1:B3,&quot;&gt;0&quot;)</f><v>8</v></c><c r="M1"><f>COUNTIF(A1:A3,&quot;C*&quot;)</f><v>0</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>dog</t></is></c><c r="B2"><v>20</v></c></row><row r="3"><c r="A3" t="inlineStr"><is><t>car</t></is></c><c r="B3"><v>30</v></c></row><row r="4"><c r="A4" t="inlineStr"><is><t>c*at</t></is></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>COUNTIF(A1:A3,&quot;c*&quot;)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMIF(A1:A3,&quot;c*&quot;)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>AVERAGEIF(A1:A3,&quot;c*&quot;)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMIFS(B1:B3,A1:A3,&quot;c*&quot;)</f><v>40</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTIF(A4:A4,&quot;c~*&quot;)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTIF(A4:A4,&quot;c~**&quot;)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTIF(A1:A3,&quot;&lt;&gt;d*&quot;)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTIF(A1:A3,&quot;&gt;apple&quot;)</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMIF(A1:A3,&quot;ab&quot;)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>SUMIFS(B1:B3,A1:A3,&quot;c*&quot;,B1:B3,&quot;&gt;0&quot;)</f><v>8</v>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTIF(A1:A3,&quot;C*&quot;)</f><v>2</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_whatif() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><f>B1*2</f><v>0</v></c><c r="B1"><v>1</v></c><c r="C1"><v>3</v></c><c r="D1"><f>WHATIF(A1,B1,C1:C3)</f><v>0</v></c><c r="E1"><f>WHATIF(A1,B1,C1:C3)+1</f><v>9</v></c><c r="F1"><f>WHATIF(A1,A1,C1)</f><v>8</v></c><c r="G1"><f>WHATIF(A1,B1,G2:G3)</f><v>7</v></c><c r="H1"><f>WHATIF(H1,B1,C1)</f><v>4</v></c><c r="I1"><f>T(&quot;ab&quot;)</f><v>0</v></c><c r="J1"><f>WHATIF(I1,B1,C1)</f><v>6</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="C2"><v>4</v></c><c r="D2"><v>0</v></c><c r="G2"><v>1</v></c></row><row r="3"><c r="C3"><v>5</v></c><c r="D3"><v>0</v></c><c r="G3" t="inlineStr"><is><t>x</t></is></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>WHATIF(A1,B1,C1:C3)</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="D2"><v>8</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="D3"><v>10</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>WHATIF(A1,B1,C1:C3)+1</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WHATIF(A1,A1,C1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WHATIF(A1,B1,G2:G3)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WHATIF(H1,B1,C1)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WHATIF(I1,B1,C1)</f><v>6</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_whatif_grid() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><f>B1+C1</f><v>0</v></c><c r="B1"><v>1</v></c><c r="C1"><v>10</v></c><c r="D1"><v>2</v></c><c r="E1"><v>4</v></c><c r="F1"><v>5</v></c><c r="G1"><f>WHATIF(A1,B1,D1:D2,C1,E1:F1)</f><v>0</v></c><c r="H1"><v>0</v></c><c r="I1"><f>WHATIF(A1,B1,D1:D2,C1,E1:F1)+1</f><v>9</v></c><c r="J1"><f>WHATIF(A1,B1,D1:D2,B1,E1:F1)</f><v>8</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="D2"><v>3</v></c><c r="G2"><v>0</v></c><c r="H2"><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>WHATIF(A1,B1,D1:D2,C1,E1:F1)</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="H1"><v>7</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="G2"><v>7</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="H2"><v>8</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>WHATIF(A1,B1,D1:D2,C1,E1:F1)+1</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WHATIF(A1,B1,D1:D2,B1,E1:F1)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_stacks_rows() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>2</v></c><c r="C1"><f>CHOOSEROWS(A1:B2,2)</f><v>0</v></c><c r="D1"><v>0</v></c><c r="E1"><f>CHOOSEROWS(A1:B2,-1)</f><v>0</v></c><c r="F1"><v>0</v></c><c r="G1"><f>CHOOSEROWS(A1:B2,3)</f><v>9</v></c><c r="H1"><f>HSTACK(A1:A2,B1:B2)</f><v>0</v></c><c r="I1"><v>0</v></c><c r="J1"><f>VSTACK(A1:B1,A2:B2)</f><v>0</v></c><c r="K1"><v>0</v></c><c r="L1"><f>HSTACK(A1:A2,B1:B1)</f><v>8</v></c><c r="M1"><f>HSTACK(A1:B1,B1:B1)+0</f><v>7</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><v>3</v></c><c r="B2"><v>4</v></c><c r="H2"><v>0</v></c><c r="I2"><v>0</v></c><c r="J2"><v>0</v></c><c r="K2"><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>CHOOSEROWS(A1:B2,2)</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="D1"><v>4</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>CHOOSEROWS(A1:B2,-1)</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="F1"><v>4</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>CHOOSEROWS(A1:B2,3)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HSTACK(A1:A2,B1:B2)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="I1"><v>2</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="H2"><v>3</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="I2"><v>4</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>VSTACK(A1:B1,A2:B2)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="K1"><v>2</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="J2"><v>3</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="K2"><v>4</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>HSTACK(A1:A2,B1:B1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HSTACK(A1:B1,B1:B1)+0</f><v>7</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_take() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>10</v></c><c r="C1"><f>TAKE(A1:A4,2)</f><v>0</v></c><c r="D1"><f>TAKE(A1:A4,-2)</f><v>0</v></c><c r="E1"><f>DROP(A1:A4,1)</f><v>0</v></c><c r="F1"><v>1</v></c><c r="G1"><v>10</v></c><c r="H1"><f>CHOOSECOLS(F1:G2,2)</f><v>0</v></c><c r="I1"><f>TAKE(F1:G2,1,2)</f><v>0</v></c><c r="J1"><v>0</v></c><c r="K1"><f>TAKE(A1:A4,0)</f><v>5</v></c><c r="L1"><f>TAKE(A1:A4,2)+1</f><v>6</v></c><c r="M1"><f>CHOOSECOLS(F1:G2,3)</f><v>7</v></c><c r="N1"><f>DROP(A1:A4,4)</f><v>8</v></c><c r="O1"><f>TAKE(A1:A4,1,2)</f><v>9</v></c><c r="P1"><f>TAKE(Q1:Q2,1)</f><v>11</v></c><c r="Q1"><v>1</v></c><c r="R1"><f>DROP(A1:A4,-1)</f><v>0</v></c><c r="S1"><f>CHOOSECOLS(F1:G2,-1)</f><v>0</v></c><c r="T1"><f>DROP(F1:G2,0,1)</f><v>0</v></c><c r="U1"><f>TAKE(F1:G2,-1,-1)</f><v>0</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><v>2</v></c><c r="C2"><v>0</v></c><c r="D2"><v>0</v></c><c r="E2"><v>0</v></c><c r="F2"><v>2</v></c><c r="G2"><v>20</v></c><c r="H2"><v>0</v></c><c r="Q2" t="inlineStr"><is><t>x</t></is></c><c r="R2"><v>0</v></c><c r="S2"><v>0</v></c><c r="T2"><v>0</v></c></row><row r="3"><c r="A3"><v>3</v></c><c r="E3"><v>0</v></c><c r="R3"><v>0</v></c></row><row r="4"><c r="A4"><v>4</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>TAKE(A1:A4,2)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="C2"><v>2</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>TAKE(A1:A4,-2)</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="D2"><v>4</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>DROP(A1:A4,1)</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="E2"><v>3</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="E3"><v>4</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>CHOOSECOLS(F1:G2,2)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="H2"><v>20</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>TAKE(F1:G2,1,2)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="J1"><v>10</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>TAKE(A1:A4,0)</f><v>5</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>TAKE(A1:A4,2)+1</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHOOSECOLS(F1:G2,3)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>DROP(A1:A4,4)</f><v>8</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>TAKE(A1:A4,1,2)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TAKE(Q1:Q2,1)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DROP(A1:A4,-1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="R2"><v>2</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="R3"><v>3</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>CHOOSECOLS(F1:G2,-1)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="S2"><v>20</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>DROP(F1:G2,0,1)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="T2"><v>20</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>TAKE(F1:G2,-1,-1)</f><v>20</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_growth() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>1</v></c><c r="C1"><v>4</v></c><c r="D1"><f>GROWTH(A1:A3,B1:B3,C1:C2)</f><v>0</v></c><c r="E1"><f>LOGEST(A1:A3,B1:B3)</f><v>0</v></c><c r="F1"><v>0</v></c><c r="G1"><f>GROWTH(H1:H3,I1:I3,J1)</f><v>3</v></c><c r="H1"><v>1</v></c><c r="I1"><v>1</v></c><c r="J1"><v>4</v></c><c r="K1"><f>LOGEST(A1:A3,B1:B3,0)</f><v>4</v></c><c r="L1"><f>LOGEST(A1:A3,B1:B3,1,1)</f><v>5</v></c><c r="M1"><v>1</v></c><c r="N1"><v>1</v></c><c r="O1"><f>LOGEST(M1:M3,N1:N3,1,1)</f><v>9</v></c><c r="P1"><v>9</v></c><c r="Q1"><f>GROWTH(A1:A3,B1:B3,C1:C2)+1</f><v>6</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><v>2</v></c><c r="B2"><v>2</v></c><c r="C2"><v>5</v></c><c r="D2"><v>0</v></c><c r="H2"><v>0</v></c><c r="I2"><v>2</v></c><c r="M2"><v>2</v></c><c r="N2"><v>2</v></c><c r="O2"><v>9</v></c><c r="P2"><v>9</v></c></row><row r="3"><c r="A3"><v>4</v></c><c r="B3"><v>3</v></c><c r="H3"><v>4</v></c><c r="I3"><v>3</v></c><c r="M3"><v>5</v></c><c r="N3"><v>3</v></c><c r="O3"><v>9</v></c><c r="P3"><v>9</v></c></row><row r="4"><c r="O4"><v>9</v></c><c r="P4"><v>9</v></c></row><row r="5"><c r="O5"><v>9</v></c><c r="P5"><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>GROWTH(A1:A3,B1:B3,C1:C2)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="D2"><v>16</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>LOGEST(A1:A3,B1:B3)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="F1"><v>0.5</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>GROWTH(H1:H3,I1:I3,J1)</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGEST(A1:A3,B1:B3,0)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGEST(A1:A3,B1:B3,1,1)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GROWTH(A1:A3,B1:B3,C1:C2)+1</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGEST(M1:M3,N1:N3,1,1)</f><v>2.23606798</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="P1"><v>0.43088694</v></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="O2"><v>0.06441599</v></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="P2"><v>0.13915445</v></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="O3"><v>0.99363314</v></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="P3"><v>0.09109797</v></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="O4"><v>156.06338719</v></c>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="P4"><v>1</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<c r="O5"><v>1.2951452</v></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="P5"><v>0.00829884</v></c>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_array_constant() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><f>SUM({1,2;3,4})</f><v>0</v></c><c r="A2"><f>AVERAGE({1,2,3})</f><v>0</v></c><c r="A3"><f>COUNT({1,&quot;a&quot;,3})</f><v>0</v></c><c r="A4"><f>COUNTA({1,&quot;a&quot;})</f><v>0</v></c><c r="A5"><f>COUNTBLANK({1,&quot;a&quot;})</f><v>11</v></c><c r="A6"><f>SUM({1,2;3})</f><v>5</v></c><c r="A7"><f>SUM({1,,2})</f><v>6</v></c><c r="A8"><f>SUM({A1})</f><v>7</v></c><c r="A9"><f>{1}</f><v>8</v></c><c r="B1"><f>SUM({1,2})+1</f><v>0</v></c><c r="B2"><f>PRODUCT({2,3})</f><v>0</v></c><c r="B3"><f>MIN({3,1,2})</f><v>0</v></c><c r="B4"><f>AVERAGEA({1,&quot;a&quot;})</f><v>0</v></c><c r="B5"><f>SUMIF({1,2},&quot;&gt;0&quot;)</f><v>9</v></c><c r="B6"><f>MAX({1,-2})</f><v>0</v></c><c r="Z1"><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SUM({1,2;3,4})</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>AVERAGE({1,2,3})</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNT({1,&quot;a&quot;,3})</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTA({1,&quot;a&quot;})</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTBLANK({1,&quot;a&quot;})</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>SUM({1,2;3})</f><v>5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SUM({1,,2})</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SUM({A1})</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>{1}</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SUM({1,2})+1</f><v>4</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>PRODUCT({2,3})</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>MIN({3,1,2})</f><v>1</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>AVERAGEA({1,&quot;a&quot;})</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMIF({1,2},&quot;&gt;0&quot;)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>MAX({1,-2})</f><v>1</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_text_split() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><f>TEXTBEFORE(&quot;red,green&quot;,&quot;,&quot;)</f><v>0</v></c><c r="A2"><f>TEXTAFTER(&quot;red,green&quot;,&quot;,&quot;)</f><v>0</v></c><c r="A3"><f>TEXTAFTER(&quot;red,green,blue&quot;,&quot;,&quot;,2)</f><v>0</v></c><c r="A4"><f>TEXTBEFORE(&quot;a*b*c&quot;,&quot;*&quot;,2)</f><v>0</v></c><c r="A5"><f>TEXTBEFORE(&quot;Ab&quot;,&quot;B&quot;)</f><v>4</v></c><c r="A6"><f>TEXTBEFORE(&quot;red&quot;,&quot;,&quot;)</f><v>5</v></c><c r="A7"><f>TEXTBEFORE(&quot;red&quot;,&quot;&quot;)</f><v>6</v></c><c r="A8"><f>TEXTBEFORE(&quot;red,green&quot;,&quot;,&quot;,0)</f><v>7</v></c><c r="B1"><f>TEXTSPLIT(&quot;red,green,blue&quot;,&quot;,&quot;)</f><v>0</v></c><c r="C1"><f>TEXTSPLIT(&quot;a-a-a-a-a-a-a-a-a-a-a-a-a-a-a-a-a&quot;,&quot;-&quot;)</f><v>9</v></c><c r="D1"><f>TEXTSPLIT(&quot;red,green&quot;,&quot;,&quot;)</f><v>0</v></c><c r="E1"><f>TEXTBEFORE(&quot;café,x&quot;,&quot;é&quot;)</f><v>0</v></c><c r="F1"><f>TEXTBEFORE(&quot;,red&quot;,&quot;,&quot;)</f><v>1</v></c><c r="G1"><v>2</v></c><c r="H1"><f>TEXTSPLIT(&quot;a,b&quot;,&quot;,&quot;,&quot;;&quot;)</f><v>10</v></c><c r="I1"><f>TEXTSPLIT(&quot;red,green&quot;,&quot;,&quot;)+1</f><v>8</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A9"><f>TEXTAFTER(&quot;a-b-c&quot;,&quot;-&quot;,G1)</f><v>0</v></c><c r="B2"><v>1</v></c><c r="C2"><v>2</v></c><c r="D2"><f>FOO()</f><v>3</v></c></row><row r="3"><c r="B3"><v>1</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(
                r#"<f>TEXTBEFORE(&quot;red,green&quot;,&quot;,&quot;)</f><is><t>red</t></is>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>TEXTAFTER(&quot;red,green&quot;,&quot;,&quot;)</f><is><t>green</t></is>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXTAFTER(&quot;red,green,blue&quot;,&quot;,&quot;,2)</f><is><t>blue</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>TEXTBEFORE(&quot;a*b*c&quot;,&quot;*&quot;,2)</f><is><t>a*b</t></is>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXTBEFORE(&quot;Ab&quot;,&quot;B&quot;)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXTBEFORE(&quot;red&quot;,&quot;,&quot;)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXTBEFORE(&quot;red&quot;,&quot;&quot;)</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXTBEFORE(&quot;red,green&quot;,&quot;,&quot;,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="B1" t="inlineStr"><f>TEXTSPLIT(&quot;red,green,blue&quot;,&quot;,&quot;)</f><is><t>red</t></is></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="B2" t="inlineStr"><is><t>green</t></is></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="B3" t="inlineStr"><is><t>blue</t></is></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXTSPLIT(&quot;a-a-a-a-a-a-a-a-a-a-a-a-a-a-a-a-a&quot;,&quot;-&quot;)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="C2"><v>2</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>TEXTSPLIT(&quot;red,green&quot;,&quot;,&quot;)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>FOO()</f><v>3</v>"#), "{sheet}");
        assert!(
            sheet.contains(
                r#"<f>TEXTBEFORE(&quot;café,x&quot;,&quot;é&quot;)</f><is><t>caf</t></is>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXTBEFORE(&quot;,red&quot;,&quot;,&quot;)</f><is><t></t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>TEXTAFTER(&quot;a-b-c&quot;,&quot;-&quot;,G1)</f><is><t>c</t></is>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>TEXTSPLIT(&quot;a,b&quot;,&quot;,&quot;,&quot;;&quot;)</f><v>10</v>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXTSPLIT(&quot;red,green&quot;,&quot;,&quot;)+1</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_indirect() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/><sheet name="Other" sheetId="2" r:id="rId2"/><sheet name="My Sheet" sheetId="3" r:id="rId3"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Target="worksheets/sheet2.xml"/><Relationship Id="rId3" Target="worksheets/sheet3.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><f>C1+1</f><v>0</v></c><c r="B1"><v>4</v></c><c r="C1"><f>1+1</f><v>8</v></c><c r="D1" t="inlineStr"><is><t>A2</t></is></c><c r="E1"><f>INDIRECT(&quot;A1&quot;)</f><v>0</v></c><c r="F1"><f>INDIRECT(&quot;Budgets!A1&quot;)</f><v>0</v></c><c r="G1"><f>INDIRECT(&quot;Other!A1&quot;)</f><v>0</v></c><c r="H1"><f>SUM(INDIRECT(&quot;A1:A2&quot;))</f><v>0</v></c><c r="I1"><f>INDIRECT(&quot;A1:A2&quot;)</f><v>5</v></c><c r="J1"><f>INDIRECT(&quot;R1C1&quot;)</f><v>6</v></c><c r="K1"><f>INDIRECT(&quot;[Book]A1&quot;)</f><v>7</v></c><c r="L1"><f>OFFSET(A1,1,0)</f><v>0</v></c><c r="M1"><f>SUM(OFFSET(A1,0,0,2,1))</f><v>0</v></c><c r="N1"><f>OFFSET(A1,-1,0)</f><v>8</v></c><c r="O1"><f>SUM(OFFSET(INDIRECT(&quot;A1&quot;),0,0,2,1))</f><v>0</v></c><c r="P1"><f>INDIRECT(&quot;$A$2&quot;)</f><v>0</v></c><c r="Q1"><f>INDIRECT(D1)</f><v>0</v></c><c r="R1"><f>OFFSET(Budgets!A1,0,0)</f><v>0</v></c><c r="S1"><f>SUM(OFFSET(A1:A2,0,1))</f><v>0</v></c><c r="T1"><f>INDIRECT(&quot;'My Sheet'!A1&quot;)</f><v>0</v></c><c r="U1"><f>INDIRECT(&quot;Missing!A1&quot;)</f><v>11</v></c><c r="V1"><f>OFFSET(A1,0,0,0,1)</f><v>12</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><v>20</v></c></row></sheetData></worksheet>"#,
            ),
            (
                "xl/worksheets/sheet2.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><f>1+1</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
            (
                "xl/worksheets/sheet3.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>4</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>INDIRECT(&quot;A1&quot;)</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>INDIRECT(&quot;Budgets!A1&quot;)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>INDIRECT(&quot;Other!A1&quot;)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUM(INDIRECT(&quot;A1:A2&quot;))</f><v>23</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>INDIRECT(&quot;A1:A2&quot;)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>INDIRECT(&quot;R1C1&quot;)</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>INDIRECT(&quot;[Book]A1&quot;)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>OFFSET(A1,1,0)</f><v>20</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUM(OFFSET(A1,0,0,2,1))</f><v>23</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>OFFSET(A1,-1,0)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUM(OFFSET(INDIRECT(&quot;A1&quot;),0,0,2,1))</f><v>23</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>INDIRECT(&quot;$A$2&quot;)</f><v>20</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>INDIRECT(D1)</f><v>20</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>OFFSET(Budgets!A1,0,0)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUM(OFFSET(A1:A2,0,1))</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>INDIRECT(&quot;'My Sheet'!A1&quot;)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>INDIRECT(&quot;Missing!A1&quot;)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>OFFSET(A1,0,0,0,1)</f><v>12</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_passes_other_sheets() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/><sheet name="Other" sheetId="2" r:id="rId2"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Target="worksheets/sheet2.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><f>1+1</f><v>5</v></c><c r="B1"><f>A1+1</f><v>0</v></c><c r="C1"><f>Other!A1</f><v>0</v></c><c r="D1"><f>Other!B1</f><v>0</v></c><c r="E1"><f>Budgets!B1</f><v>0</v></c><c r="F1"><f>B1</f><v>0</v></c><c r="G1"><f>Other!C1</f><v>0</v></c><c r="H1"><f>Other!D1</f><v>0</v></c><c r="I1"><f>Other!E1</f><v>0</v></c><c r="J1"><f>Other!F1</f><v>0</v></c><c r="Z1"><v>0</v></c></row></sheetData></worksheet>"#,
            ),
            (
                "xl/worksheets/sheet2.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><f>1+1</f><v>8</v></c><c r="B1"><f>A1+1</f><v>0</v></c><c r="C1"><f>1/0</f><v>7</v></c><c r="D1"><f>&quot;ab&quot;</f><v>0</v></c><c r="E1"><f>Budgets!B1</f><v>0</v></c><c r="F1"><f>FREQUENCY(A1:A1,B1:B1)</f><v>4</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>Other!A1</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Other!B1</f><v>9</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Budgets!B1</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>B1</f><v>3</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Other!C1</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains("<t>ab</t>"), "{sheet}");
        assert!(sheet.contains(r#"<f>Other!E1</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Other!F1</f><v>4</v>"#), "{sheet}");
        let other = read_entry(&mut archive, "xl/worksheets/sheet2.xml").unwrap();
        assert!(other.contains(r#"<f>1+1</f><v>8</v>"#), "{other}");
    }

    #[test]
    fn set_sheet_cell_shared_and_other_sheet() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/><sheet name="Other" sheetId="2" r:id="rId2"/><sheet name="My Sheet" sheetId="3" r:id="rId3"/><sheet name="Bob's" sheetId="4" r:id="rId4"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Target="worksheets/sheet2.xml"/><Relationship Id="rId3" Target="worksheets/sheet3.xml"/><Relationship Id="rId4" Target="worksheets/sheet4.xml"/></Relationships>"#,
            ),
            ("xl/sharedStrings.xml", r#"<sst><si><t>Cat</t></si></sst>"#),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="M1"><f>Q1+1</f><v>9</v></c><c r="P1"><v>2</v></c><c r="Q1"><f>1+1</f><v>5</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="B2"><f>LEN(A1)</f><v>0</v></c><c r="C2"><f>MATCH(&quot;c*&quot;,A1:A1,0)</f><v>0</v></c><c r="D2"><f>Other!A1</f><v>0</v></c><c r="E2"><f>'My Sheet'!A1</f><v>0</v></c><c r="F2"><f>'Bob''s'!A1</f><v>0</v></c><c r="G2"><f>Other!B1</f><v>0</v></c><c r="H2"><f>Other!C1</f><v>5</v></c><c r="I2"><f>Other!A1:A2</f><v>6</v></c><c r="J2"><f>Missing!A1</f><v>3</v></c><c r="K2"><f>Budgets!M1</f><v>0</v></c><c r="L2"><f>M1</f><v>0</v></c><c r="N2"><f>IRR(A1:P1)</f><v>11</v></c><c r="O2"><f>INDEX(A1:A1,1)</f><v>0</v></c><c r="Q2"><f>AVERAGEA(A1,P1)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
            (
                "xl/worksheets/sheet2.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>7</v></c><c r="B1"><f>1+1</f><v>9</v></c><c r="C1"><f>1+1</f></c></row></sheetData></worksheet>"#,
            ),
            (
                "xl/worksheets/sheet3.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>4</v></c></row></sheetData></worksheet>"#,
            ),
            (
                "xl/worksheets/sheet4.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>LEN(A1)</f><v>3</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>MATCH(&quot;c*&quot;,A1:A1,0)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>Other!A1</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>'My Sheet'!A1</f><v>4</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>'Bob''s'!A1</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Other!B1</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Other!C1</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Other!A1:A2</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Missing!A1</f><v>3</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Budgets!M1</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>M1</f><v>3</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>IRR(A1:P1)</f><v>11</v>"#), "{sheet}");
        assert!(
            sheet.contains(
                r#"<c r="O2" t="inlineStr"><f>INDEX(A1:A1,1)</f><is><t>Cat</t></is></c>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>AVERAGEA(A1,P1)</f><v>1</v>"#),
            "{sheet}"
        );
        let other = read_entry(&mut archive, "xl/worksheets/sheet2.xml").unwrap();
        assert!(other.contains(r#"<f>1+1</f><v>9</v>"#), "{other}");
        assert!(other.contains(r#"<c r="C1"><f>1+1</f></c>"#), "{other}");
    }

    #[test]
    fn set_sheet_cell_spills_a_short_result() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>2</v></c><c r="G1"><v>1</v></c><c r="J1"><v>1</v></c><c r="K1"><v>1</v></c><c r="P1"><v>4</v></c><c r="S1" t="inlineStr"><is><t>Cat</t></is></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><v>2</v></c><c r="B2"><v>4</v></c><c r="G2"><v>2</v></c><c r="J2"><v>2</v></c><c r="K2"><v>2</v></c><c r="P2"><v>5</v></c></row><row r="3"><c r="A3"><v>3</v></c><c r="B3"><v>5</v></c><c r="G3"><v>2</v></c><c r="J3" t="inlineStr"><is><t>Cat</t></is></c><c r="K3"><v>9</v></c></row><row r="4"><c r="A4"><v>4</v></c><c r="B4"><v>1</v></c><c r="G4"><v>3</v></c><c r="J4"><v>3</v></c><c r="K4"><v>3</v></c></row><row r="5"><c r="G5"><v>3</v></c></row><row r="6"><c r="C6"><f>FREQUENCY(A1:A4,B1:B2)</f><v>9</v></c><c r="E6"><f>FREQUENCY(A1:A4,B1:B2)</f><v>9</v></c><c r="F6"><f>FREQUENCY(A1:A4,B1:B2)</f><v>9</v></c><c r="H6"><f>MODE.MULT(G1:G5)</f><v>0</v></c><c r="I6"><f>MODE.MULT(A1:A4)</f><v>6</v></c><c r="L6"><f>LINEST(J1:J4,K1:K4)</f><v>0</v></c><c r="M6"><v>9</v></c><c r="O6"><f>TREND(J1:J4,K1:K4,P1:P2)</f><v>0</v></c><c r="Q6"><f>TREND(J1:J4,K1:K4,P1:P2)</f><v>3</v></c><c r="R6"><f>FREQUENCY(S1:S1,B1:B1)</f><v>5</v></c><c r="T6"><f>FREQUENCY(A1:A4,B3:B4)</f><v>6</v></c><c r="U6"><f>FREQUENCY(A1:A4,B1:B2)+0</f><v>7</v></c><c r="X6"><f>LINEST(J1:J4,K1:K4)</f><v>4</v></c></row><row r="7"><c r="C7"><v>8</v></c><c r="E7"><v>8</v></c><c r="F7"><f>FOO()</f><v>4</v></c><c r="H7"><v>0</v></c><c r="O7"><v>8</v></c><c r="Q7"><f>FOO()</f><v>6</v></c></row><row r="8"><c r="C8"><v>7</v></c><c r="F8"><v>3</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<c r="C6"><f>FREQUENCY(A1:A4,B1:B2)</f><v>2</v></c>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="C7"><v>2</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="C8"><v>0</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<c r="E6"><f>FREQUENCY(A1:A4,B1:B2)</f><v>2</v></c>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="E7"><v>2</v></c>"#), "{sheet}");
        assert!(!sheet.contains(r#"r="E8""#), "{sheet}");
        assert!(
            sheet.contains(r#"<c r="F6"><f>FREQUENCY(A1:A4,B1:B2)</f><v>2</v></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="F7"><f>FOO()</f><v>4</v></c>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="F8"><v>3</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>MODE.MULT(G1:G5)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="H7"><v>3</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>MODE.MULT(A1:A4)</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="L6"><f>LINEST(J1:J4,K1:K4)</f><v>1</v></c>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="M6"><v>0</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<c r="X6"><f>LINEST(J1:J4,K1:K4)</f><v>4</v></c>"#),
            "{sheet}"
        );
        assert!(!sheet.contains(r#"r="Y6""#), "{sheet}");
        assert!(
            sheet.contains(r#"<c r="O6"><f>TREND(J1:J4,K1:K4,P1:P2)</f><v>4</v></c>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="O7"><v>5</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<c r="Q6"><f>TREND(J1:J4,K1:K4,P1:P2)</f><v>3</v></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="Q7"><f>FOO()</f><v>6</v></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FREQUENCY(S1:S1,B1:B1)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FREQUENCY(A1:A4,B3:B4)</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FREQUENCY(A1:A4,B1:B2)+0</f><v>7</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_interest_and_dated_cash() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><v>-10000</v></c><c r="B2"><v>39448</v></c><c r="C2"><v>-120000</v></c></row><row r="3"><c r="A3"><v>2750</v></c><c r="B3"><v>39508</v></c><c r="C3"><v>39000</v></c></row><row r="4"><c r="A4"><v>4250</v></c><c r="B4"><v>39751</v></c><c r="C4"><v>30000</v></c></row><row r="5"><c r="A5"><v>3250</v></c><c r="B5"><v>39859</v></c><c r="C5"><v>21000</v></c></row><row r="6"><c r="A6"><v>2750</v></c><c r="B6"><v>39904</v></c><c r="C6"><v>37000</v></c></row><row r="7"><c r="C7"><v>46000</v></c></row><row r="8"><c r="A8"><f>IPMT(0.1/12,1,36,20000)</f><v>0</v></c><c r="B8"><f>PPMT(0.1/12,1,36,20000)</f><v>0</v></c><c r="C8"><f>IPMT(0.1,1,3,8000,0,1)</f><v>0</v></c><c r="D8"><f>CUMIPMT(0.09/12,360,125000,13,24,0)</f><v>0</v></c><c r="E8"><f>CUMPRINC(0.09/12,360,125000,13,24,0)</f><v>0</v></c><c r="F8"><f>IPMT(0.1,0,3,8000)</f><v>9</v></c><c r="G8"><f>XNPV(0.09,A2:A6,B2:B6)</f><v>0</v></c><c r="H8"><f>XIRR(A2:A6,B2:B6)</f><v>0</v></c><c r="I8"><f>MIRR(C2:C7,0.1,0.12)</f><v>0</v></c><c r="J8"><f>XIRR(A2:A2,B2:B2)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>IPMT(0.1/12,1,36,20000)</f><v>-166.66666667</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PPMT(0.1/12,1,36,20000)</f><v>-478.67707721</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>IPMT(0.1,1,3,8000,0,1)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CUMIPMT(0.09/12,360,125000,13,24,0)</f><v>-11135.23213075</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CUMPRINC(0.09/12,360,125000,13,24,0)</f><v>-934.10712342</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>IPMT(0.1,0,3,8000)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>XNPV(0.09,A2:A6,B2:B6)</f><v>2086.64760203</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>XIRR(A2:A6,B2:B6)</f><v>0.37336253</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MIRR(C2:C7,0.1,0.12)</f><v>0.12609413</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>XIRR(A2:A2,B2:B2)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_text_format() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Cat</t></is></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><f>TEXT(1234.5,&quot;0.00&quot;)</f><v>0</v></c><c r="B2"><f>TEXT(1234.5,&quot;#,##0.00&quot;)</f><v>0</v></c><c r="C2"><f>TEXT(0.25,&quot;0%&quot;)</f><v>0</v></c><c r="D2"><f>TEXT(1234.5,&quot;0.##&quot;)</f><v>0</v></c><c r="E2"><f>TEXT(43831,&quot;yyyy-mm-dd&quot;)</f><v>0</v></c><c r="F2"><f>TEXT(43831,&quot;d/m/yy&quot;)</f><v>0</v></c><c r="G2"><f>TEXT(A1,&quot;@&quot;)</f><v>0</v></c><c r="H2"><f>TEXT(1,&quot;0.00E+00&quot;)</f><v>9</v></c><c r="I2"><f>TEXT(-0.004,&quot;0.00&quot;)</f><v>0</v></c><c r="J2"><f>TEXT(12,&quot;&quot;&quot;id &quot;&quot;0&quot;)</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>TEXT(1234.5,&quot;0.00&quot;)</f><is><t>1234.50</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(1234.5,&quot;#,##0.00&quot;)</f><is><t>1,234.50</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(0.25,&quot;0%&quot;)</f><is><t>25%</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(1234.5,&quot;0.##&quot;)</f><is><t>1234.5</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet
                .contains(r#"<f>TEXT(43831,&quot;yyyy-mm-dd&quot;)</f><is><t>2020-01-01</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(43831,&quot;d/m/yy&quot;)</f><is><t>1/1/20</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(A1,&quot;@&quot;)</f><is><t>Cat</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(1,&quot;0.00E+00&quot;)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(-0.004,&quot;0.00&quot;)</f><is><t>-0.00</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>TEXT(12,&quot;&quot;&quot;id &quot;&quot;0&quot;)</f><is><t>id 12</t></is>"#
            ),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_convert_units() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="Z1"><v>0</v></c><c r="A1"><f>CONVERT(1,&quot;ft&quot;,&quot;in&quot;)</f><v>0</v></c><c r="B1"><f>CONVERT(1,&quot;kg&quot;,&quot;g&quot;)</f><v>0</v></c><c r="C1"><f>CONVERT(1,&quot;hr&quot;,&quot;mn&quot;)</f><v>0</v></c><c r="D1"><f>CONVERT(0,&quot;C&quot;,&quot;F&quot;)</f><v>0</v></c><c r="E1"><f>CONVERT(100,&quot;C&quot;,&quot;K&quot;)</f><v>0</v></c><c r="F1"><f>CONVERT(1,&quot;yr&quot;,&quot;day&quot;)</f><v>0</v></c><c r="G1"><f>CONVERT(1,&quot;mi&quot;,&quot;km&quot;)</f><v>0</v></c><c r="H1"><f>CONVERT(1,&quot;m&quot;,&quot;kg&quot;)</f><v>4</v></c><c r="I1"><f>CONVERT(-300,&quot;C&quot;,&quot;K&quot;)</f><v>5</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>CONVERT(1,&quot;ft&quot;,&quot;in&quot;)</f><v>12</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONVERT(1,&quot;kg&quot;,&quot;g&quot;)</f><v>1000</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONVERT(1,&quot;hr&quot;,&quot;mn&quot;)</f><v>60</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONVERT(0,&quot;C&quot;,&quot;F&quot;)</f><v>32</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONVERT(100,&quot;C&quot;,&quot;K&quot;)</f><v>373.15</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONVERT(1,&quot;yr&quot;,&quot;day&quot;)</f><v>365.25</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONVERT(1,&quot;mi&quot;,&quot;km&quot;)</f><v>1.609344</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONVERT(1,&quot;m&quot;,&quot;kg&quot;)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONVERT(-300,&quot;C&quot;,&quot;K&quot;)</f><v>5</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_date1904() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><workbookPr date1904="1"/><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="Z1"><v>0</v></c><c r="A1"><f>DATE(1904,1,1)</f><v>9</v></c><c r="B1"><f>YEAR(0)</f><v>0</v></c><c r="C1"><f>MONTH(0)</f><v>0</v></c><c r="D1"><f>DAY(0)</f><v>0</v></c><c r="E1"><f>WEEKDAY(0)</f><v>0</v></c><c r="F1"><f>WEEKDAY(0,2)</f><v>0</v></c><c r="G1"><f>EDATE(0,1)</f><v>0</v></c><c r="H1"><f>NETWORKDAYS(0,6)</f><v>0</v></c><c r="I1"><f>DATE(1900,1,1)</f><v>8</v></c><c r="J1"><f>TEXT(0,&quot;yyyy-mm-dd&quot;)</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>DATE(1904,1,1)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>YEAR(0)</f><v>1904</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MONTH(0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>DAY(0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>WEEKDAY(0)</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>WEEKDAY(0,2)</f><v>5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>EDATE(0,1)</f><v>31</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>NETWORKDAYS(0,6)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DATE(1900,1,1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(0,&quot;yyyy-mm-dd&quot;)</f><is><t>1904-01-01</t></is>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_linest_stats() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>2</v></c><c r="B1"><v>1</v></c><c r="C1"><f>LINEST(A1:A4,B1:B4,1,1)</f><v>0</v></c><c r="D1"><v>0</v></c><c r="E1"><f>LINEST(A1:A4,B1:B4,0)</f><v>7</v></c><c r="H1"><f>LINEST(A1:A4,B1:B4,1,0)</f><v>0</v></c><c r="I1"><v>0</v></c><c r="K1"><f>LINEST(A1:A2,B1:B2,1,1)</f><v>4</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><v>3</v></c><c r="B2"><v>2</v></c><c r="C2"><v>0</v></c><c r="D2"><v>0</v></c><c r="H2"><v>9</v></c></row><row r="3"><c r="A3"><v>5</v></c><c r="B3"><v>3</v></c><c r="C3"><v>0</v></c><c r="D3"><v>0</v></c></row><row r="4"><c r="A4"><v>4</v></c><c r="B4"><v>4</v></c><c r="C4"><v>0</v></c><c r="D4"><v>0</v></c></row><row r="5"><c r="C5"><v>0</v></c><c r="D5"><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<c r="C1"><f>LINEST(A1:A4,B1:B4,1,1)</f><v>0.8</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="D1"><v>1.5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="C2"><v>0.42426407</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="D2"><v>1.161895</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="C3"><v>0.64</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="D3"><v>0.9486833</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="C4"><v>3.55555556</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="D4"><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="C5"><v>3.2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="D5"><v>1.8</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>LINEST(A1:A4,B1:B4,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LINEST(A1:A4,B1:B4,1,0)</f><v>0.8</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="I1"><v>1.5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="H2"><v>9</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>LINEST(A1:A2,B1:B2,1,1)</f><v>4</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_defined_names() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/><sheet name="Other" sheetId="2" r:id="rId2"/><sheet name="My Sheet" sheetId="3" r:id="rId3"/></sheets><definedNames><definedName name="Sales">Budgets!$A$1:$A$2</definedName><definedName name="Rate">Budgets!$B$1</definedName><definedName name="Live">Budgets!$C$1</definedName><definedName name="Abroad">Other!$A$1:$A$2</definedName><definedName name="OtherTotal">Other!$A$1</definedName><definedName name="Quoted">'My Sheet'!$A$1</definedName><definedName name="Local" localSheetId="0">Budgets!$B$1</definedName><definedName name="Plus">Budgets!$A$1+1</definedName></definedNames></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Target="worksheets/sheet2.xml"/><Relationship Id="rId3" Target="worksheets/sheet3.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>10</v></c><c r="A2"><v>20</v></c><c r="B1"><v>3</v></c><c r="C1"><f>1+1</f><v>8</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="D2"><f>SUM(sales)</f><v>0</v></c><c r="E2"><f>Rate*2</f><v>0</v></c><c r="F2"><f>Live</f><v>0</v></c><c r="G2"><f>OtherTotal</f><v>0</v></c><c r="H2"><f>SUM(Abroad)</f><v>0</v></c><c r="I2"><f>Quoted</f><v>0</v></c><c r="J2"><f>Local</f><v>5</v></c><c r="K2"><f>Plus</f><v>4</v></c><c r="L2"><f>SUMIF(Sales,&quot;&gt;15&quot;)</f><v>0</v></c><c r="M2"><f>COUNTA(Sales)</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
            (
                "xl/worksheets/sheet2.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><f>1+1</f><v>9</v></c></row><row r="2"><c r="A2"><v>4</v></c></row></sheetData></worksheet>"#,
            ),
            (
                "xl/worksheets/sheet3.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>4</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>SUM(sales)</f><v>30</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Rate*2</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Live</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>OtherTotal</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SUM(Abroad)</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Quoted</f><v>4</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Local</f><v>3</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Plus</f><v>11</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>SUMIF(Sales,&quot;&gt;15&quot;)</f><v>20</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>COUNTA(Sales)</f><v>2</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_local_name() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/><sheet name="Other" sheetId="2" r:id="rId2"/></sheets><definedNames><definedName name="Rate">Budgets!$B$1</definedName><definedName name="Rate" localSheetId="0">Budgets!$A$1</definedName><definedName name="Only" localSheetId="1">Other!$A$1</definedName><definedName name="Plus" localSheetId="0">Budgets!$A$1+1</definedName><definedName name="Gone" localSheetId="5">Budgets!$A$1</definedName></definedNames></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Target="worksheets/sheet2.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>10</v></c><c r="B1"><v>3</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="D2"><f>Rate</f><v>0</v></c><c r="E2"><f>Rate*2</f><v>0</v></c><c r="F2"><f>Only</f><v>4</v></c><c r="G2"><f>Plus</f><v>0</v></c><c r="H2"><f>SUM(Rate)</f><v>0</v></c><c r="I2"><f>Gone</f><v>6</v></c></row></sheetData></worksheet>"#,
            ),
            (
                "xl/worksheets/sheet2.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>7</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="B2"><f>Rate</f><v>0</v></c><c r="C2"><f>Only</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>Rate</f><v>10</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Rate*2</f><v>20</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Only</f><v>4</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Plus</f><v>11</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SUM(Rate)</f><v>10</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Gone</f><v>6</v>"#), "{sheet}");
        let other = set_sheet_cell(&bytes, "Other", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(other)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet2.xml").unwrap();
        assert!(sheet.contains(r#"<f>Rate</f><v>3</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Only</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_shared_formula() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="B1"><v>2</v></c><c r="A1"><f t="shared" ref="A1:A3" si="0">B1*2</f><v>0</v></c><c r="C1"><f t="shared" ref="C1:C2" si="1">$B1+B$1</f><v>0</v></c><c r="D1"><f>B1+1</f><v>0</v></c><c r="H1"><f t="shared" ref="H1:H2" si="2">LEN(&quot;A1&quot;)</f><v>0</v></c><c r="F1"><f t="shared" si="3"/><v>8</v></c><c r="G1"><f t="array" ref="G1">1+1</f><v>6</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="B2"><v>3</v></c><c r="A2"><f t="shared" si="0"/><v>0</v></c><c r="C2"><f t="shared" si="1"/><v>0</v></c><c r="H2"><f t="shared" si="2"/><v>0</v></c><c r="F2"><f t="shared" ref="F2:F1" si="3">A1</f><v>0</v></c></row><row r="3"><c r="B3"><v>4</v></c><c r="A3"><f t="shared" si="0"/><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f t="shared" ref="A1:A3" si="0">B1*2</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f t="shared" si="0"/><v>6</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"si="0"/><v>8</v>"#)
                || sheet.contains(r#"<c r="A3"><f t="shared" si="0"/><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f t="shared" ref="C1:C2" si="1">$B1+B$1</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f t="shared" si="1"/><v>5</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>B1+1</f><v>3</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f t="shared" ref="H1:H2" si="2">LEN(&quot;A1&quot;)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f t="shared" si="2"/><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f t="shared" si="3"/><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f t="array" ref="G1">1+1</f><v>6</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_bond_price() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="Z1"><v>0</v></c><c r="A1"><f>PRICE(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,100,2,0)</f><v>0</v></c><c r="B1"><f>YIELD(DATE(2008,2,15),DATE(2017,11,15),0.0575,94.63436162,100,2)</f><v>0</v></c><c r="C1"><f>DURATION(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,2)</f><v>0</v></c><c r="D1"><f>MDURATION(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,2)</f><v>0</v></c><c r="E1"><f>PRICE(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,100,4)</f><v>9</v></c><c r="F1"><f>PRICE(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,100,2,1)</f><v>8</v></c><c r="G1"><f>PRICE(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,100,3)</f><v>7</v></c><c r="H1"><f>PRICE(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,100,2,2)</f><v>6</v></c><c r="N1"><f>PRICE(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,100,2,3)</f><v>3</v></c><c r="O1"><f>PRICE(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,100,2,4)</f><v>2</v></c><c r="P1"><f>PRICE(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,100,2,5)</f><v>5</v></c><c r="Q1"><f>DURATION(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,2,2)</f><v>0</v></c><c r="R1"><f>PRICE(DATE(2008,1,31),DATE(2017,11,15),0.0575,0.065,100,2,0)</f><v>0</v></c><c r="S1"><f>PRICE(DATE(2008,1,31),DATE(2017,11,15),0.0575,0.065,100,2,4)</f><v>0</v></c><c r="T1"><f>YIELD(DATE(2008,2,15),DATE(2017,11,15),0.0575,94.60241718,100,2,2)</f><v>0</v></c><c r="I1"><f>PRICE(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,100,4,1)</f><v>4</v></c><c r="J1"><f>YIELD(DATE(2008,2,15),DATE(2017,11,15),0.0575,94.63544921,100,2,1)</f><v>0</v></c><c r="K1"><f>DURATION(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,4)</f><v>0</v></c><c r="L1"><f>MDURATION(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,2,1)</f><v>0</v></c><c r="M1"><f>YIELD(DATE(2008,2,15),DATE(2017,11,15),0.0575,94.61509395,100,4)</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>PRICE(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,100,2,0)</f><v>94.63436162</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>YIELD(DATE(2008,2,15),DATE(2017,11,15),0.0575,94.63436162,100,2)</f><v>0.065</v>"#) || sheet.contains("<v>0.065"), "{sheet}");
        assert!(
            sheet.contains(r#"<f>DURATION(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,2)</f><v>7.4164847</v>"#)
                || sheet.contains(r#"<v>7.41648469</v>"#)
                || sheet.contains(r#"<v>7.41648470</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>MDURATION(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,2)</f><v>7.183"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>PRICE(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,100,4)</f><v>94.61509395</v>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>PRICE(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,100,2,1)</f><v>94.63544921</v>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>PRICE(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,100,3)</f><v>7</v>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>PRICE(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,100,2,2)</f><v>94.60241718</v>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>PRICE(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,100,4,1)</f><v>94.61509395</v>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>YIELD(DATE(2008,2,15),DATE(2017,11,15),0.0575,94.63544921,100,2,1)</f><v>0.065</v>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>DURATION(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,4)</f><v>7.45611478</v>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>MDURATION(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,2,1)</f><v>7.18037525</v>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>YIELD(DATE(2008,2,15),DATE(2017,11,15),0.0575,94.61509395,100,4)</f><v>0.065</v>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>PRICE(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,100,2,5)</f><v>5</v>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>PRICE(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,100,2,3)</f><v>94.64359455</v>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>PRICE(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,100,2,4)</f><v>94.63436162</v>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>DURATION(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,2,2)</f><v>7.4164847</v>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>PRICE(DATE(2008,1,31),DATE(2017,11,15),0.0575,0.065,100,2,0)</f><v>94.60225776</v>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>PRICE(DATE(2008,1,31),DATE(2017,11,15),0.0575,0.065,100,2,4)</f><v>94.61822998</v>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>YIELD(DATE(2008,2,15),DATE(2017,11,15),0.0575,94.60241718,100,2,2)</f><v>0.065</v>"#
            ),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_text_elapsed() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="Z1"><v>0</v></c><c r="A1"><f>TEXT(1,&quot;yyyy-mm-dd&quot;)</f><v>0</v></c><c r="B1"><f>TEXT(1.5,&quot;[h]&quot;)</f><v>0</v></c><c r="C1"><f>TEXT(1.5,&quot;[m]&quot;)</f><v>0</v></c><c r="D1"><f>TEXT(1/24,&quot;[hh]&quot;)</f><v>0</v></c><c r="E1"><f>TEXT(-1.5,&quot;[h]&quot;)</f><v>0</v></c><c r="F1"><f>TEXT(1.5,&quot;[h]:mm&quot;)</f><v>4</v></c><c r="G1"><f>TEXT(1000000,&quot;[h]&quot;)</f><v>5</v></c><c r="H1"><f>TEXT(1,&quot;[mm]&quot;)</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>TEXT(1,&quot;yyyy-mm-dd&quot;)</f><is><t>1900-01-01</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(1.5,&quot;[h]&quot;)</f><is><t>36</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(1.5,&quot;[m]&quot;)</f><is><t>2160</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(1/24,&quot;[hh]&quot;)</f><is><t>01</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(-1.5,&quot;[h]&quot;)</f><is><t>-36</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(1.5,&quot;[h]:mm&quot;)</f><is><t>36:00</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(1000000,&quot;[h]&quot;)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(1,&quot;[mm]&quot;)</f><is><t>1440</t></is>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_text_names() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="Z1"><v>0</v></c><c r="A1"><f>TEXT(1,&quot;dddd&quot;)</f><v>0</v></c><c r="B1"><f>TEXT(1,&quot;ddd&quot;)</f><v>0</v></c><c r="C1"><f>TEXT(1,&quot;mmmm&quot;)</f><v>0</v></c><c r="D1"><f>TEXT(1,&quot;mmm&quot;)</f><v>0</v></c><c r="E1"><f>TEXT(61,&quot;dddd&quot;)</f><v>0</v></c><c r="F1"><f>TEXT(1,&quot;yyyy-mmm-dd&quot;)</f><v>0</v></c><c r="G1"><f>TEXT(1,&quot;[h]:mm&quot;)</f><v>4</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>TEXT(1,&quot;dddd&quot;)</f><is><t>Monday</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(1,&quot;ddd&quot;)</f><is><t>Mon</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(1,&quot;mmmm&quot;)</f><is><t>January</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(1,&quot;mmm&quot;)</f><is><t>Jan</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(61,&quot;dddd&quot;)</f><is><t>Friday</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(1,&quot;yyyy-mmm-dd&quot;)</f><is><t>1900-Jan-01</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(1,&quot;[h]:mm&quot;)</f><is><t>24:00</t></is>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_text_clock() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="Z1"><v>0</v></c><c r="A1"><f>TEXT(1.5,&quot;hh:mm&quot;)</f><v>0</v></c><c r="B1"><f>TEXT(1.5,&quot;h:mm:ss&quot;)</f><v>0</v></c><c r="C1"><f>TEXT(1+1/86400,&quot;hh:mm:ss&quot;)</f><v>0</v></c><c r="D1"><f>TEXT(1,&quot;yyyy-mm-dd hh:mm&quot;)</f><v>0</v></c><c r="E1"><f>TEXT(1,&quot;mm:ss&quot;)</f><v>0</v></c><c r="F1"><f>TEXT(1.75,&quot;h&quot;)</f><v>0</v></c><c r="G1"><f>TEXT(1.5,&quot;[h]:mm&quot;)</f><v>4</v></c><c r="H1"><f>TEXT(1.5,&quot;hh:mm AM&quot;)</f><v>5</v></c></row><row r="2"><c r="A2"><f>TEXT(0.25,&quot;h:mm AM&quot;)</f><v>0</v></c><c r="B2"><f>TEXT(1.5,&quot;[h]:mm:ss&quot;)</f><v>0</v></c><c r="C2"><f>TEXT(-1.5,&quot;[h]:mm&quot;)</f><v>0</v></c><c r="D2"><f>TEXT(1.5,&quot;yyyy-mm-dd hh:mm AM&quot;)</f><v>0</v></c><c r="E2"><f>TEXT(1/24,&quot;[hh]:mm&quot;)</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>TEXT(1.5,&quot;hh:mm&quot;)</f><is><t>12:00</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(1.5,&quot;h:mm:ss&quot;)</f><is><t>12:00:00</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet
                .contains(r#"<f>TEXT(1+1/86400,&quot;hh:mm:ss&quot;)</f><is><t>00:00:01</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>TEXT(1,&quot;yyyy-mm-dd hh:mm&quot;)</f><is><t>1900-01-01 00:00</t></is>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(1,&quot;mm:ss&quot;)</f><is><t>00:00</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(1.75,&quot;h&quot;)</f><is><t>18</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(1.5,&quot;[h]:mm&quot;)</f><is><t>36:00</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(1.5,&quot;hh:mm AM&quot;)</f><is><t>12:00 PM</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(0.25,&quot;h:mm AM&quot;)</f><is><t>6:00 AM</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(1.5,&quot;[h]:mm:ss&quot;)</f><is><t>36:00:00</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(-1.5,&quot;[h]:mm&quot;)</f><is><t>-36:00</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>TEXT(1.5,&quot;yyyy-mm-dd hh:mm AM&quot;)</f><is><t>1900-01-01 12:00 PM</t></is>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(1/24,&quot;[hh]:mm&quot;)</f><is><t>01:00</t></is>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_text_sections() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="Z1"><v>0</v></c><c r="A1"><f>TEXT(12.5,&quot;0.00;&quot;&quot;neg&quot;&quot;;0&quot;)</f><v>0</v></c><c r="B1"><f>TEXT(-2,&quot;0.00;&quot;&quot;neg&quot;&quot;;0&quot;)</f><v>0</v></c><c r="C1"><f>TEXT(0,&quot;0.00;&quot;&quot;neg&quot;&quot;;0&quot;)</f><v>0</v></c><c r="D1"><f>TEXT(-3,&quot;0.00;0.00&quot;)</f><v>0</v></c><c r="E1"><f>TEXT(0,&quot;0.00;0.00&quot;)</f><v>0</v></c><c r="F1"><f>TEXT(-4,&quot;0.00;;0&quot;)</f><v>0</v></c><c r="G1"><f>TEXT(1,&quot;0;[Red]0&quot;)</f><v>5</v></c><c r="H1"><f>TEXT(3,&quot;&quot;&quot;a;b&quot;&quot;0&quot;)</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>TEXT(12.5,&quot;0.00;&quot;&quot;neg&quot;&quot;;0&quot;)</f><is><t>12.50</t></is>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>TEXT(-2,&quot;0.00;&quot;&quot;neg&quot;&quot;;0&quot;)</f><is><t>neg</t></is>"#), "{sheet}");
        assert!(
            sheet.contains(
                r#"<f>TEXT(0,&quot;0.00;&quot;&quot;neg&quot;&quot;;0&quot;)</f><is><t>0</t></is>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(-3,&quot;0.00;0.00&quot;)</f><is><t>3.00</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(0,&quot;0.00;0.00&quot;)</f><is><t>0.00</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(-4,&quot;0.00;;0&quot;)</f><is><t></t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(1,&quot;0;[Red]0&quot;)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>TEXT(3,&quot;&quot;&quot;a;b&quot;&quot;0&quot;)</f><is><t>a;b3</t></is>"#
            ),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_three_d() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/><sheet name="Other" sheetId="2" r:id="rId2"/><sheet name="My Sheet" sheetId="3" r:id="rId3"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Target="worksheets/sheet2.xml"/><Relationship Id="rId3" Target="worksheets/sheet3.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>10</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><f>A3+1</f><v>8</v></c><c r="B2"><f>SUM(Budgets:Other!A1)</f><v>0</v></c><c r="C2"><f>SUM(Other:'My Sheet'!A1)</f><v>0</v></c><c r="D2"><f>SUM(Budgets:Budgets!A2)</f><v>0</v></c><c r="E2"><f>A2</f><v>0</v></c><c r="F2"><f>SUM(Nope:Other!A1)</f><v>6</v></c></row><row r="3"><c r="A3"><f>1+1</f><v>5</v></c></row></sheetData></worksheet>"#,
            ),
            (
                "xl/worksheets/sheet2.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>7</v></c></row></sheetData></worksheet>"#,
            ),
            (
                "xl/worksheets/sheet3.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>3</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SUM(Budgets:Other!A1)</f><v>17</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUM(Other:'My Sheet'!A1)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUM(Budgets:Budgets!A2)</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>A2</f><v>3</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>SUM(Nope:Other!A1)</f><v>6</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_iterates_a_cycle() {
        let sheet = r#"<worksheet><sheetData><row r="1"><c r="A1"><f>B1+1</f><v>0</v></c><c r="B1"><f>A1+1</f><v>0</v></c><c r="Z1"><v>0</v></c></row></sheetData></worksheet>"#;
        let plain = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            ("xl/worksheets/sheet1.xml", sheet),
        ]);
        let iterated = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><calcPr iterate="1" iterateCount="1" iterateDelta="0.001"/><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            ("xl/worksheets/sheet1.xml", sheet),
        ]);
        let plain = set_sheet_cell(&plain, "Budgets", "Z1", "1").unwrap();
        let iterated = set_sheet_cell(&iterated, "Budgets", "Z1", "1").unwrap();
        let mut plain_zip = ZipArchive::new(Cursor::new(plain)).unwrap();
        let mut iterated_zip = ZipArchive::new(Cursor::new(iterated)).unwrap();
        let plain_sheet = read_entry(&mut plain_zip, "xl/worksheets/sheet1.xml").unwrap();
        let iterated_sheet = read_entry(&mut iterated_zip, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            plain_sheet.contains(r#"<f>B1+1</f><v>0</v>"#),
            "{plain_sheet}"
        );
        assert!(
            plain_sheet.contains(r#"<f>A1+1</f><v>0</v>"#),
            "{plain_sheet}"
        );
        assert!(
            iterated_sheet.contains(r#"<f>B1+1</f><v>2</v>"#),
            "{iterated_sheet}"
        );
        assert!(
            iterated_sheet.contains(r#"<f>A1+1</f><v>2</v>"#),
            "{iterated_sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_iterates_across_sheets() {
        let budgets = r#"<worksheet><sheetData><row r="1"><c r="A1"><f>Abroad!A1+1</f><v>0</v></c><c r="Z1"><v>0</v></c></row></sheetData></worksheet>"#;
        let abroad = r#"<worksheet><sheetData><row r="1"><c r="A1"><f>Budgets!A1+1</f><v>0</v></c></row></sheetData></worksheet>"#;
        let book = |calc: &str| {
            zip_bytes(&[
                ("xl/workbook.xml", calc),
                (
                    "xl/_rels/workbook.xml.rels",
                    r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Target="worksheets/sheet2.xml"/></Relationships>"#,
                ),
                ("xl/worksheets/sheet1.xml", abroad),
                ("xl/worksheets/sheet2.xml", budgets),
            ])
        };
        let plain = book(
            r#"<workbook><sheets><sheet name="Abroad" sheetId="1" r:id="rId1"/><sheet name="Budgets" sheetId="2" r:id="rId2"/></sheets></workbook>"#,
        );
        let iterated = book(
            r#"<workbook><calcPr iterate="1" iterateCount="2" iterateDelta="0.001"/><sheets><sheet name="Abroad" sheetId="1" r:id="rId1"/><sheet name="Budgets" sheetId="2" r:id="rId2"/></sheets></workbook>"#,
        );
        let plain = set_sheet_cell(&plain, "Budgets", "Z1", "1").unwrap();
        let iterated = set_sheet_cell(&iterated, "Budgets", "Z1", "1").unwrap();
        let mut plain_zip = ZipArchive::new(Cursor::new(plain)).unwrap();
        let mut iterated_zip = ZipArchive::new(Cursor::new(iterated)).unwrap();
        let plain_sheet = read_entry(&mut plain_zip, "xl/worksheets/sheet2.xml").unwrap();
        let iterated_sheet = read_entry(&mut iterated_zip, "xl/worksheets/sheet2.xml").unwrap();
        let abroad = read_entry(&mut iterated_zip, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            plain_sheet.contains(r#"<f>Abroad!A1+1</f><v>2</v>"#),
            "{plain_sheet}"
        );
        assert!(
            iterated_sheet.contains(r#"<f>Abroad!A1+1</f><v>3</v>"#),
            "{iterated_sheet}"
        );
        assert!(
            abroad.contains(r#"<f>Budgets!A1+1</f><v>0</v>"#),
            "{abroad}"
        );
    }

    #[test]
    fn set_sheet_cell_formula_name() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets><definedNames><definedName name="Plus">Budgets!$A$1+1</definedName><definedName name="Two">1+1</definedName><definedName name="Relative">Budgets!A1+1</definedName><definedName name="Nested">Plus+1</definedName></definedNames></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><f>1+1</f><v>8</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="D2"><f>Plus</f><v>0</v></c><c r="E2"><f>Plus*2</f><v>0</v></c><c r="F2"><f>SUM(Plus)</f><v>0</v></c><c r="G2"><f>Two</f><v>0</v></c><c r="H2"><f>Relative</f><v>4</v></c><c r="I2"><f>Nested</f><v>5</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>Plus</f><v>3</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Plus*2</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SUM(Plus)</f><v>3</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Two</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Relative</f><v>4</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Nested</f><v>5</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_table_column() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/><sheet name="Other" sheetId="2" r:id="rId2"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Target="worksheets/sheet2.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/table" Target="../tables/table1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/table" Target="../tables/table3.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/_rels/sheet2.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/table" Target="../tables/table2.xml"/></Relationships>"#,
            ),
            (
                "xl/tables/table1.xml",
                r#"<table name="Sales" displayName="Sales" ref="A1:B3"><tableColumns count="2"><tableColumn id="1" name="Item"/><tableColumn id="2" name="Amount"/></tableColumns></table>"#,
            ),
            (
                "xl/tables/table2.xml",
                r#"<table name="Totals" displayName="Totals" ref="C1:C2"><tableColumns count="1"><tableColumn id="1" name="N"/></tableColumns></table>"#,
            ),
            (
                "xl/tables/table3.xml",
                r#"<table name="Tiny" displayName="Tiny" ref="D10:D11"><tableColumns count="1"><tableColumn id="1" name="Value"/></tableColumns></table>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Item</t></is></c><c r="B1" t="inlineStr"><is><t>Amount</t></is></c><c r="D1"><f>SUM(Sales[Amount])</f><v>0</v></c><c r="E1"><f>Sales[Amount]</f><v>5</v></c><c r="F1"><f>SUM(Sales[[#This Row],[Amount]])</f><v>6</v></c><c r="G1"><f>SUM(Sales[Nope])</f><v>8</v></c><c r="H1"><f>SUM(Totals[N])</f><v>0</v></c><c r="J1"><f>Tiny[Value]</f><v>0</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>apple</t></is></c><c r="B2"><f>5+5</f><v>9</v></c><c r="D2"><f>SUM(sales[amount])</f><v>0</v></c></row><row r="3"><c r="A3" t="inlineStr"><is><t>pear</t></is></c><c r="B3"><v>4</v></c></row><row r="10"><c r="D10" t="inlineStr"><is><t>Value</t></is></c></row><row r="11"><c r="D11"><v>2</v></c></row></sheetData></worksheet>"#,
            ),
            (
                "xl/worksheets/sheet2.xml",
                r#"<worksheet><sheetData><row r="1"><c r="C1" t="inlineStr"><is><t>N</t></is></c></row><row r="2"><c r="C2"><f>1+1</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SUM(Sales[Amount])</f><v>14</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUM(sales[amount])</f><v>14</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>Sales[Amount]</f><v>5</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>SUM(Sales[[#This Row],[Amount]])</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUM(Sales[Nope])</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUM(Totals[N])</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>Tiny[Value]</f><v>2</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_let_names() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets><definedNames><definedName name="Rate">Budgets!$B$1</definedName></definedNames></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>10</v></c><c r="B1"><v>3</v></c><c r="D1"><f>LET(x,A1+1,x*2)</f><v>0</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="D2"><f>LET(x,1,x,x+1,x)</f><v>0</v></c></row><row r="3"><c r="D3"><f>LET(x,1,LET(y,x+1,y))</f><v>0</v></c></row><row r="4"><c r="D4"><f>LET(x,&quot;ab&quot;,x)</f><v>0</v></c></row><row r="5"><c r="D5"><f>LET(x,Rate,x)</f><v>5</v></c></row><row r="6"><c r="D6"><f>LET(A1,1,A1)</f><v>6</v></c></row><row r="7"><c r="D7"><f>LET(x,1,y,2,z,3,a,4,b,5,c,6,d,7,e,8,e)</f><v>0</v></c></row><row r="8"><c r="D8"><f>LET(x,1,y,2,z,3,a,4,b,5,c,6,d,7,e,8,f,9,f)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>LET(x,A1+1,x*2)</f><v>22</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LET(x,1,x,x+1,x)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LET(x,1,LET(y,x+1,y))</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains("<t>ab</t>"), "{sheet}");
        assert!(sheet.contains(r#"<f>LET(x,Rate,x)</f><v>5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>LET(A1,1,A1)</f><v>6</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>LET(x,1,y,2,z,3,a,4,b,5,c,6,d,7,e,8,e)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LET(x,1,y,2,z,3,a,4,b,5,c,6,d,7,e,8,f,9,f)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_xlookup_row() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>apple</t></is></c><c r="B1"><v>10</v></c><c r="C1"><v>1</v></c><c r="E1"><f>XLOOKUP(&quot;pear&quot;,A1:A2,B1:C2)</f><v>0</v></c><c r="F1"><v>0</v></c><c r="G1"><f>XLOOKUP(&quot;nope&quot;,A1:A2,B1:C2,9)</f><v>0</v></c><c r="H1"><v>4</v></c><c r="I1"><f>XLOOKUP(&quot;nope&quot;,A1:A2,B1:C2)</f><v>6</v></c><c r="J1"><f>XLOOKUP(&quot;pear&quot;,A1:A2,B1:C2)+1</f><v>5</v></c><c r="K1"><f>XLOOKUP(&quot;apple&quot;,A1:A2,B1:B2)</f><v>0</v></c><c r="L1"><f>XLOOKUP(&quot;a*&quot;,A1:A2,B1:C2,7,1)</f><v>11</v></c><c r="M1"><v>8</v></c><c r="N1"><f>XLOOKUP(&quot;pear&quot;,A1:A2,P1:Q2)</f><v>3</v></c><c r="O1"><v>0</v></c><c r="P1"><v>8</v></c><c r="Q1"><v>9</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>pear</t></is></c><c r="B2"><v>20</v></c><c r="C2"><v>2</v></c><c r="P2"><v>4</v></c><c r="Q2" t="inlineStr"><is><t>x</t></is></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>XLOOKUP(&quot;pear&quot;,A1:A2,B1:C2)</f><v>20</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="F1"><v>2</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>XLOOKUP(&quot;nope&quot;,A1:A2,B1:C2,9)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="H1"><v>4</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>XLOOKUP(&quot;nope&quot;,A1:A2,B1:C2)</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>XLOOKUP(&quot;pear&quot;,A1:A2,B1:C2)+1</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>XLOOKUP(&quot;apple&quot;,A1:A2,B1:B2)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>XLOOKUP(&quot;a*&quot;,A1:A2,B1:C2,7,1)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>XLOOKUP(&quot;pear&quot;,A1:A2,P1:Q2)</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="O1"><v>0</v></c>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_xlookup_column() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>apple</t></is></c><c r="B1" t="inlineStr"><is><t>pear</t></is></c><c r="C1" t="inlineStr"><is><t>plum</t></is></c><c r="E1"><f>XLOOKUP(&quot;pear&quot;,A1:C1,A2:C4)</f><v>0</v></c><c r="F1"><f>XLOOKUP(&quot;nope&quot;,A1:C1,A2:C4,9)</f><v>0</v></c><c r="G1"><f>XLOOKUP(&quot;nope&quot;,A1:C1,A2:C4)</f><v>6</v></c><c r="H1"><f>XLOOKUP(&quot;pear&quot;,A1:C1,A2:C4)+1</f><v>5</v></c><c r="I1"><f>XLOOKUP(&quot;pear&quot;,A1:C1,A5:C5)</f><v>0</v></c><c r="J1"><f>XLOOKUP(&quot;plum&quot;,A1:C1,A2:C4)</f><v>3</v></c><c r="K1"><f>XLOOKUP(&quot;a*&quot;,A1:C1,A2:C4,7,1)</f><v>11</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><v>1</v></c><c r="B2"><v>10</v></c><c r="C2" t="inlineStr"><is><t>x</t></is></c><c r="E2"><v>0</v></c><c r="F2"><v>4</v></c></row><row r="3"><c r="A3"><v>2</v></c><c r="B3"><v>20</v></c><c r="C3"><v>8</v></c><c r="E3"><v>0</v></c></row><row r="4"><c r="A4"><v>3</v></c><c r="B4"><v>30</v></c><c r="C4"><v>9</v></c></row><row r="5"><c r="A5"><v>4</v></c><c r="B5"><v>40</v></c><c r="C5"><v>50</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>XLOOKUP(&quot;pear&quot;,A1:C1,A2:C4)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="E2"><v>20</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="E3"><v>30</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>XLOOKUP(&quot;nope&quot;,A1:C1,A2:C4,9)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="F2"><v>4</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>XLOOKUP(&quot;nope&quot;,A1:C1,A2:C4)</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>XLOOKUP(&quot;pear&quot;,A1:C1,A2:C4)+1</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>XLOOKUP(&quot;pear&quot;,A1:C1,A5:C5)</f><v>40</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>XLOOKUP(&quot;plum&quot;,A1:C1,A2:C4)</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>XLOOKUP(&quot;a*&quot;,A1:C1,A2:C4,7,1)</f><v>11</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_xlookup() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>apple</t></is></c><c r="B1"><v>10</v></c><c r="D1"><f>XLOOKUP(&quot;apple&quot;,A1:A4,B1:B4)</f><v>0</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>pear</t></is></c><c r="B2"><v>20</v></c><c r="D2"><f>XLOOKUP(&quot;pear&quot;,A1:A4,B1:B4)</f><v>0</v></c></row><row r="3"><c r="A3" t="inlineStr"><is><t>apple</t></is></c><c r="B3"><v>30</v></c><c r="D3"><f>XLOOKUP(&quot;nope&quot;,A1:A4,B1:B4,&quot;miss&quot;)</f><v>0</v></c></row><row r="4"><c r="A4"><v>4</v></c><c r="B4"><v>40</v></c><c r="D4"><f>XLOOKUP(&quot;nope&quot;,A1:A4,B1:B4)</f><v>5</v></c></row><row r="5"><c r="D5"><f>XLOOKUP(4,A1:A4,B1:B4)</f><v>0</v></c></row><row r="6"><c r="D6"><f>XMATCH(&quot;Apple&quot;,A1:A4)</f><v>0</v></c></row><row r="7"><c r="D7"><f>XMATCH(&quot;nope&quot;,A1:A4)</f><v>6</v></c></row><row r="8"><c r="D8"><f>XLOOKUP(&quot;a*&quot;,A1:A4,B1:B4)</f><v>7</v></c></row><row r="9"><c r="D9"><f>XMATCH(&quot;pear&quot;,A1:A4,1)</f><v>8</v></c></row><row r="10"><c r="D10"><f>XLOOKUP(&quot;pear&quot;,A1:A4,B1:B3)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>XLOOKUP(&quot;apple&quot;,A1:A4,B1:B4)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>XLOOKUP(&quot;pear&quot;,A1:A4,B1:B4)</f><v>20</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains("<t>miss</t>"), "{sheet}");
        assert!(
            sheet.contains(r#"<f>XLOOKUP(&quot;nope&quot;,A1:A4,B1:B4)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>XLOOKUP(4,A1:A4,B1:B4)</f><v>40</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>XMATCH(&quot;Apple&quot;,A1:A4)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>XMATCH(&quot;nope&quot;,A1:A4)</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>XLOOKUP(&quot;a*&quot;,A1:A4,B1:B4)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>XMATCH(&quot;pear&quot;,A1:A4,1)</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>XLOOKUP(&quot;pear&quot;,A1:A4,B1:B3)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_xlookup_mode() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="E1"><v>1</v></c><c r="F1"><v>10</v></c><c r="G1"><f>XLOOKUP(4,E1:E3,F1:F3,&quot;miss&quot;,1)</f><v>0</v></c><c r="H1"><f>XMATCH(4,E1:E3,1)</f><v>0</v></c><c r="I1"><f>XMATCH(4,E1:E3,-1)</f><v>9</v></c><c r="J1"><f>XMATCH(4,E1:E3,2)</f><v>8</v></c><c r="L1"><v>5</v></c><c r="M1"><v>50</v></c><c r="N1"><f>XLOOKUP(4,L1:L3,M1:M3,&quot;miss&quot;,-1)</f><v>0</v></c><c r="A1" t="inlineStr"><is><t>apple</t></is></c><c r="B1"><v>10</v></c><c r="O1"><f>XLOOKUP(&quot;a*&quot;,A1:A2,B1:B2,&quot;miss&quot;,1)</f><v>7</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="E2"><v>3</v></c><c r="F2"><v>30</v></c><c r="L2"><v>3</v></c><c r="M2"><v>30</v></c><c r="A2" t="inlineStr"><is><t>pear</t></is></c><c r="B2"><v>20</v></c></row><row r="3"><c r="E3"><v>5</v></c><c r="F3"><v>50</v></c><c r="L3"><v>1</v></c><c r="M3"><v>10</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>XLOOKUP(4,E1:E3,F1:F3,&quot;miss&quot;,1)</f><v>30</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>XMATCH(4,E1:E3,1)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>XMATCH(4,E1:E3,-1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>XMATCH(4,E1:E3,2)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>XLOOKUP(4,L1:L3,M1:M3,&quot;miss&quot;,-1)</f><v>50</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>XLOOKUP(&quot;a*&quot;,A1:A2,B1:B2,&quot;miss&quot;,1)</f><v>7</v>"#
            ),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_sorts_text() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>pear</t></is></c><c r="B1"><f>SORT(A1:A3)</f><v>0</v></c><c r="C1"><f>SORT(A1:A3,-1)</f><v>0</v></c><c r="D1"><v>3</v></c><c r="E1"><v>10</v></c><c r="F1" t="inlineStr"><is><t>pear</t></is></c><c r="G1"><f>SORTBY(D1:E2,F1:F2)</f><v>0</v></c><c r="H1"><v>0</v></c><c r="J1"><f>FILTER(D1:E2,F1:F2,&quot;pear&quot;)</f><v>0</v></c><c r="K1"><v>0</v></c><c r="L1"><f>FILTER(D1:E2,F1:F2,&quot;p*&quot;)</f><v>0</v></c><c r="M1"><v>0</v></c><c r="N1"><f>FILTER(D1:E2,F1:F2,&quot;zz&quot;)</f><v>6</v></c><c r="P1"><f>SORT(Q1:Q2)</f><v>4</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>apple</t></is></c><c r="Q2"><v>1</v></c><c r="B2"><v>0</v></c><c r="C2"><v>0</v></c><c r="D2"><v>1</v></c><c r="E2"><v>20</v></c><c r="F2" t="inlineStr"><is><t>apple</t></is></c><c r="G2"><v>0</v></c><c r="H2"><v>0</v></c><c r="J2"><v>0</v></c><c r="K2"><v>0</v></c></row><row r="3"><c r="A3" t="inlineStr"><is><t>Pear</t></is></c><c r="Q1" t="inlineStr"><is><t>apple</t></is></c><c r="B3"><v>0</v></c><c r="C3"><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SORT(A1:A3)</f><is><t>apple</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="B2" t="inlineStr"><is><t>pear</t></is></c>"#)
                || sheet.contains(r#"<c r="B2" t="inlineStr"><is><t>Pear</t></is></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SORT(A1:A3,-1)</f><is><t>pear</t></is>"#)
                || sheet.contains(r#"<f>SORT(A1:A3,-1)</f><is><t>Pear</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="C3" t="inlineStr"><is><t>apple</t></is></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SORTBY(D1:E2,F1:F2)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="H1"><v>20</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="G2"><v>3</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="H2"><v>10</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>FILTER(D1:E2,F1:F2,&quot;pear&quot;)</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="K1"><v>10</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>FILTER(D1:E2,F1:F2,&quot;p*&quot;)</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FILTER(D1:E2,F1:F2,&quot;zz&quot;)</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>SORT(Q1:Q2)</f><v>4</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_sorts_a_column() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>3</v></c><c r="E1"><v>1</v></c><c r="B1"><f>SORT(A1:A5)</f><v>0</v></c><c r="C1"><f>SORT(A1:A5,-1)</f><v>0</v></c><c r="D1"><f>UNIQUE(A1:A5)</f><v>0</v></c><c r="F1"><f>FILTER(A1:A5,E1:E5,1)</f><v>0</v></c><c r="G1"><f>SORT(A1:A5,2)</f><v>8</v></c><c r="H1"><f>FILTER(A1:A5,E1:E5,9)</f><v>6</v></c><c r="J1"><f>SORT(A6:A6)</f><v>7</v></c><c r="K1"><f>SORT(A1:A3)+1</f><v>4</v></c><c r="M1"><f>SORT(A1:A2)</f><v>9</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><v>1</v></c><c r="E2"><v>0</v></c><c r="B2"><v>0</v></c><c r="C2"><v>0</v></c><c r="D2"><v>0</v></c><c r="F2"><v>0</v></c><c r="M2"><f>FOO()</f><v>5</v></c></row><row r="3"><c r="A3"><v>3</v></c><c r="E3"><v>1</v></c><c r="B3"><v>0</v></c><c r="C3"><v>0</v></c><c r="D3"><v>0</v></c><c r="F3"><v>0</v></c></row><row r="4"><c r="E4"><v>0</v></c><c r="B4"><v>0</v></c><c r="C4"><v>0</v></c></row><row r="5"><c r="A5"><v>2</v></c><c r="E5"><v>1</v></c></row><row r="6"><c r="A6" t="inlineStr"><is><t>x</t></is></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>SORT(A1:A5)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="B2"><v>2</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="B3"><v>3</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="B4"><v>3</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>SORT(A1:A5,-1)</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="C2"><v>3</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="C3"><v>2</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="C4"><v>1</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>UNIQUE(A1:A5)</f><v>3</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="D2"><v>1</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="D3"><v>2</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>FILTER(A1:A5,E1:E5,1)</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="F2"><v>3</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="F3"><v>2</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SORT(A1:A5,2)</f><v>8</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>FILTER(A1:A5,E1:E5,9)</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SORT(A6:A6)</f><is><t>x</t></is>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>SORT(A1:A3)+1</f><v>4</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SORT(A1:A2)</f><v>9</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>FOO()</f><v>5</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_sorts_a_rectangle() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>3</v></c><c r="B1"><v>10</v></c><c r="C1"><v>1</v></c><c r="D1"><f>SORT(A1:B3,2)</f><v>0</v></c><c r="E1"><v>0</v></c><c r="F1"><f>FILTER(A1:B3,C1:C3,1)</f><v>0</v></c><c r="G1"><v>0</v></c><c r="H1"><f>SORT(A1:B3)</f><v>0</v></c><c r="I1"><v>0</v></c><c r="J1"><f>SORT(A1:B3,1,-1)</f><v>0</v></c><c r="K1"><v>0</v></c><c r="L1"><f>SORT(A1:B3,3)</f><v>8</v></c><c r="M1"><f>SORT(A1:B3,-1)</f><v>7</v></c><c r="N1"><f>SORT(A1:B4)</f><v>6</v></c><c r="O1"><f>SORT(A5:B5)</f><v>5</v></c><c r="P1"><f>FILTER(A1:B3,C1:C3,9)</f><v>4</v></c><c r="Q1"><f>FILTER(A1:B3,C1:C3,1,1)</f><v>3</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><v>1</v></c><c r="B2"><v>20</v></c><c r="C2"><v>0</v></c><c r="D2"><v>0</v></c><c r="E2"><v>0</v></c><c r="F2"><v>0</v></c><c r="G2"><v>0</v></c><c r="H2"><v>0</v></c><c r="I2"><v>0</v></c><c r="J2"><v>0</v></c><c r="K2"><v>0</v></c></row><row r="3"><c r="A3"><v>3</v></c><c r="B3"><v>5</v></c><c r="C3"><v>1</v></c><c r="D3"><v>0</v></c><c r="E3"><v>0</v></c><c r="F3"><v>9</v></c><c r="G3"><v>0</v></c><c r="H3"><v>0</v></c><c r="I3"><v>0</v></c><c r="J3"><v>0</v></c><c r="K3"><v>0</v></c></row><row r="5"><c r="A5"><v>1</v></c><c r="B5" t="inlineStr"><is><t>x</t></is></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>SORT(A1:B3,2)</f><v>3</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="E1"><v>5</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="D2"><v>3</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="E2"><v>10</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="D3"><v>1</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="E3"><v>20</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SORT(A1:B3)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="I1"><v>20</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="H2"><v>3</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="I2"><v>10</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="H3"><v>3</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="I3"><v>5</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>SORT(A1:B3,1,-1)</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="K1"><v>10</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="J2"><v>3</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="K2"><v>5</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="J3"><v>1</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="K3"><v>20</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>FILTER(A1:B3,C1:C3,1)</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="G1"><v>10</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="F2"><v>3</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="G2"><v>5</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="F3"><v>9</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SORT(A1:B3,3)</f><v>8</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>SORT(A1:B3,-1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>SORT(A1:B4)</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SORT(A5:B5)</f><v>5</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>FILTER(A1:B3,C1:C3,9)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FILTER(A1:B3,C1:C3,1,1)</f><v>3</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_sorts_mixed() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>pear</t></is></c><c r="B1"><v>20</v></c><c r="C1" t="inlineStr"><is><t>x</t></is></c><c r="E1"><f>SORT(A1:B3)</f><v>0</v></c><c r="F1"><v>0</v></c><c r="H1"><f>SORT(A1:B3,2)</f><v>0</v></c><c r="I1"><v>0</v></c><c r="K1"><f>SORT(A1:B3,1,-1)</f><v>0</v></c><c r="L1"><v>0</v></c><c r="N1"><f>SORT(A1:C2)</f><v>4</v></c><c r="O1"><f>SORT(A1:B3)+1</f><v>5</v></c><c r="P1"><v>3</v></c><c r="Q1" t="inlineStr"><is><t>pear</t></is></c><c r="R1"><f>SORT(P1:Q2,2)</f><v>0</v></c><c r="S1"><v>0</v></c><c r="T1"><f>SORT(U1:V2)</f><v>6</v></c><c r="U1" t="inlineStr"><is><t>apple</t></is></c><c r="V1"><v>1</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>apple</t></is></c><c r="B2"><v>10</v></c><c r="C2" t="inlineStr"><is><t>y</t></is></c><c r="E2"><v>0</v></c><c r="F2"><v>0</v></c><c r="H2"><v>0</v></c><c r="I2"><v>0</v></c><c r="K2"><v>0</v></c><c r="L2"><v>0</v></c><c r="P2"><v>1</v></c><c r="Q2" t="inlineStr"><is><t>apple</t></is></c><c r="R2"><v>0</v></c><c r="S2"><v>0</v></c><c r="V2"><v>2</v></c></row><row r="3"><c r="A3" t="inlineStr"><is><t>Pear</t></is></c><c r="B3"><v>30</v></c><c r="E3"><v>0</v></c><c r="F3"><v>0</v></c><c r="H3"><v>0</v></c><c r="I3"><v>0</v></c><c r="K3"><v>0</v></c><c r="L3"><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SORT(A1:B3)</f><is><t>apple</t></is>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="F1"><v>10</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<c r="E2" t="inlineStr"><is><t>pear</t></is></c>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="F2"><v>20</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<c r="E3" t="inlineStr"><is><t>Pear</t></is></c>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="F3"><v>30</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>SORT(A1:B3,2)</f><is><t>apple</t></is>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="I1"><v>10</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<c r="H2" t="inlineStr"><is><t>pear</t></is></c>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="I2"><v>20</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>SORT(A1:B3,1,-1)</f><is><t>pear</t></is>"#)
                || sheet.contains(r#"<f>SORT(A1:B3,1,-1)</f><is><t>Pear</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="K3" t="inlineStr"><is><t>apple</t></is></c>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="L3"><v>10</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SORT(A1:C2)</f><v>4</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SORT(A1:B3)+1</f><v>5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SORT(P1:Q2,2)</f><v>1</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<c r="S1" t="inlineStr"><is><t>apple</t></is></c>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="R2"><v>3</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<c r="S2" t="inlineStr"><is><t>pear</t></is></c>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>SORT(U1:V2)</f><v>6</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_goal_seek() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><f>B1*2</f><v>0</v></c><c r="B1"><v>1</v></c><c r="C1"><f>GOALSEEK(A1,B1,10)</f><v>0</v></c><c r="D1"><f>GOALSEEK(A1,A1,10)</f><v>8</v></c><c r="E1"><f>GOALSEEK(B1,A1,10)</f><v>7</v></c><c r="F1"><f>GOALSEEK(A1,B1,0)</f><v>0</v></c><c r="G1"><f>GOALSEEK(A2,B2,2)</f><v>6</v></c><c r="H1"><f>GOALSEEK(A1,B1,&quot;x&quot;)</f><v>4</v></c><c r="I1"><f>GOALSEEK(A3,B3,4)</f><v>0</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><f>B2*0+1</f><v>0</v></c><c r="B2"><v>3</v></c></row><row r="3"><c r="A3"><f>B3+1</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>GOALSEEK(A1,B1,10)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="B1"><v>1</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>GOALSEEK(A1,A1,10)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GOALSEEK(B1,A1,10)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GOALSEEK(A1,B1,0)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GOALSEEK(A2,B2,2)</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="B2"><v>3</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>GOALSEEK(A1,B1,&quot;x&quot;)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GOALSEEK(A3,B3,4)</f><v>3</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_odd_coupon() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><f>ODDFPRICE(DATE(2008,11,11),DATE(2021,3,1),DATE(2008,10,15),DATE(2009,3,1),0.0785,0.0625,100,2,1)</f><v>0</v></c><c r="B1"><f>ODDFYIELD(DATE(2008,11,11),DATE(2021,3,1),DATE(2008,10,15),DATE(2009,3,1),0.0785,113.597717474079,100,2,1)</f><v>0</v></c><c r="C1"><f>ODDLPRICE(DATE(2008,2,7),DATE(2008,6,15),DATE(2007,10,15),0.0375,0.0405,100,2,0)</f><v>0</v></c><c r="D1"><f>ODDLYIELD(DATE(2008,2,7),DATE(2008,6,15),DATE(2007,10,15),0.0375,99.8782860147213,100,2,0)</f><v>0</v></c><c r="E1"><f>ODDFPRICE(DATE(2008,11,11),DATE(2021,3,1),DATE(2008,1,15),DATE(2009,3,1),0.0785,0.0625,100,2,1)</f><v>4</v></c><c r="F1"><f>ODDLPRICE(DATE(2008,2,7),DATE(2008,6,15),DATE(2007,1,15),0.0375,0.0405,100,2,0)</f><v>5</v></c><c r="G1"><f>ODDFPRICE(DATE(2008,11,11),DATE(2021,3,1),DATE(2004,1,15),DATE(2009,3,1),0.0785,0.0625,100,2,1)</f><v>6</v></c><c r="H1"><f>ODDFPRICE(DATE(2007,5,1),DATE(2014,10,31),DATE(2007,3,24),DATE(2007,10,31),0,0.01,100,2,1)</f><v>0</v></c><c r="I1"><f>ODDFYIELD(DATE(2007,5,1),DATE(2014,10,31),DATE(2007,3,24),DATE(2007,10,31),0,92.7942029404091,100,2,1)</f><v>0</v></c><c r="J1"><f>ODDFYIELD(DATE(2008,11,11),DATE(2021,3,1),DATE(2008,1,15),DATE(2009,3,1),0.0785,113.4889408396105,100,2,1)</f><v>0</v></c><c r="Z1"><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(
                r#"<f>ODDFPRICE(DATE(2008,11,11),DATE(2021,3,1),DATE(2008,10,15),DATE(2009,3,1),0.0785,0.0625,100,2,1)</f><v>113.59771747</v>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>ODDFYIELD(DATE(2008,11,11),DATE(2021,3,1),DATE(2008,10,15),DATE(2009,3,1),0.0785,113.597717474079,100,2,1)</f><v>0.0625</v>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>ODDLPRICE(DATE(2008,2,7),DATE(2008,6,15),DATE(2007,10,15),0.0375,0.0405,100,2,0)</f><v>99.87828601</v>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>ODDLYIELD(DATE(2008,2,7),DATE(2008,6,15),DATE(2007,10,15),0.0375,99.8782860147213,100,2,0)</f><v>0.0405</v>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>ODDFPRICE(DATE(2008,11,11),DATE(2021,3,1),DATE(2008,1,15),DATE(2009,3,1),0.0785,0.0625,100,2,1)</f><v>113.48894084</v>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>ODDLPRICE(DATE(2008,2,7),DATE(2008,6,15),DATE(2007,1,15),0.0375,0.0405,100,2,0)</f><v>5</v>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>ODDFPRICE(DATE(2008,11,11),DATE(2021,3,1),DATE(2004,1,15),DATE(2009,3,1),0.0785,0.0625,100,2,1)</f><v>6</v>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>ODDFPRICE(DATE(2007,5,1),DATE(2014,10,31),DATE(2007,3,24),DATE(2007,10,31),0,0.01,100,2,1)</f><v>92.79420294</v>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>ODDFYIELD(DATE(2007,5,1),DATE(2014,10,31),DATE(2007,3,24),DATE(2007,10,31),0,92.7942029404091,100,2,1)</f><v>0.01</v>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>ODDFYIELD(DATE(2008,11,11),DATE(2021,3,1),DATE(2008,1,15),DATE(2009,3,1),0.0785,113.4889408396105,100,2,1)</f><v>0.0625</v>"#
            ),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_workday_intl() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><f>WORKDAY.INTL(1,5)</f><v>0</v></c><c r="B1"><f>WORKDAY.INTL(1,5,&quot;0000011&quot;)</f><v>0</v></c><c r="C1"><f>WORKDAY(5,1)</f><v>0</v></c><c r="D1"><f>WORKDAY.INTL(5,1,&quot;0000110&quot;)</f><v>0</v></c><c r="E1"><f>WORKDAY.INTL(1,5,&quot;1111111&quot;)</f><v>9</v></c><c r="F1"><f>WORKDAY.INTL(1,5,1)</f><v>8</v></c><c r="G1"><f>WORKDAY.INTL(1,1,&quot;0000011&quot;,B2:B2)</f><v>0</v></c><c r="H1"><f>WORKDAY.INTL(1,5,&quot;000001&quot;)</f><v>7</v></c><c r="I1"><f>WORKDAY.INTL(1,5,&quot;0000012&quot;)</f><v>6</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="B2"><v>2</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>WORKDAY.INTL(1,5)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WORKDAY.INTL(1,5,&quot;0000011&quot;)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>WORKDAY(5,1)</f><v>8</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>WORKDAY.INTL(5,1,&quot;0000110&quot;)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WORKDAY.INTL(1,5,&quot;1111111&quot;)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WORKDAY.INTL(1,5,1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WORKDAY.INTL(1,1,&quot;0000011&quot;,B2:B2)</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WORKDAY.INTL(1,5,&quot;000001&quot;)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WORKDAY.INTL(1,5,&quot;0000012&quot;)</f><v>6</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_database() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Item</t></is></c><c r="B1" t="inlineStr"><is><t>Qty</t></is></c><c r="C1" t="inlineStr"><is><t>Price</t></is></c><c r="E1" t="inlineStr"><is><t>Item</t></is></c><c r="F1" t="inlineStr"><is><t>Qty</t></is></c><c r="I1" t="inlineStr"><is><t>Item</t></is></c><c r="L1" t="inlineStr"><is><t>Qty</t></is></c><c r="P1" t="inlineStr"><is><t>Item</t></is></c><c r="Q1" t="inlineStr"><is><t>Qty</t></is></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>apple</t></is></c><c r="B2"><v>2</v></c><c r="C2"><v>5</v></c><c r="E2" t="inlineStr"><is><t>Apple</t></is></c><c r="F2" t="inlineStr"><is><t>&gt;1</t></is></c><c r="I2" t="inlineStr"><is><t>pear*</t></is></c><c r="L2" t="inlineStr"><is><t>&gt;10</t></is></c><c r="P2" t="inlineStr"><is><t>apple</t></is></c></row><row r="3"><c r="A3" t="inlineStr"><is><t>pear</t></is></c><c r="B3"><v>4</v></c><c r="C3"><v>3</v></c><c r="E3" t="inlineStr"><is><t>pear</t></is></c></row><row r="4"><c r="A4" t="inlineStr"><is><t>apple</t></is></c><c r="B4"><v>1</v></c><c r="C4"><v>9</v></c></row><row r="6"><c r="A6"><f>DSUM(A1:C4,&quot;Price&quot;,E1:F2)</f><v>0</v></c><c r="B6"><f>DAVERAGE(A1:C4,&quot;Price&quot;,E1:F2)</f><v>0</v></c><c r="C6"><f>DCOUNT(A1:C4,&quot;Price&quot;,E1:F2)</f><v>0</v></c><c r="D6"><f>DCOUNTA(A1:C4,&quot;Item&quot;,E1:F2)</f><v>0</v></c><c r="E6"><f>DMIN(A1:C4,&quot;Price&quot;,E1:F2)</f><v>0</v></c><c r="F6"><f>DMAX(A1:C4,&quot;Price&quot;,E1:F2)</f><v>0</v></c><c r="G6"><f>DSUM(A1:C4,3,E1:F2)</f><v>0</v></c><c r="H6"><f>DSUM(A1:C4,&quot;Price&quot;,E1:F3)</f><v>1</v></c><c r="I6"><f>DSUM(A1:C4,&quot;Price&quot;,I1:I2)</f><v>9</v></c><c r="J6"><f>DSUM(A1:C4,&quot;Nope&quot;,E1:F2)</f><v>7</v></c><c r="K6"><f>DSUM(A1:C4,&quot;Price&quot;,P1:Q2)</f><v>0</v></c><c r="L6"><f>DSUM(A1:C4,&quot;Price&quot;,L1:L2)</f><v>0</v></c><c r="M6"><f>DAVERAGE(A1:C4,&quot;Price&quot;,L1:L2)</f><v>4</v></c><c r="N6"><f>DSUM(A1:C4,&quot;Price&quot;,E1:F10)</f><v>6</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>DSUM(A1:C4,&quot;Price&quot;,E1:F2)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DAVERAGE(A1:C4,&quot;Price&quot;,E1:F2)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DCOUNT(A1:C4,&quot;Price&quot;,E1:F2)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DCOUNTA(A1:C4,&quot;Item&quot;,E1:F2)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DMIN(A1:C4,&quot;Price&quot;,E1:F2)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DMAX(A1:C4,&quot;Price&quot;,E1:F2)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DSUM(A1:C4,3,E1:F2)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DSUM(A1:C4,&quot;Price&quot;,E1:F3)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DSUM(A1:C4,&quot;Price&quot;,I1:I2)</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DSUM(A1:C4,&quot;Nope&quot;,E1:F2)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DSUM(A1:C4,&quot;Price&quot;,P1:Q2)</f><v>14</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DSUM(A1:C4,&quot;Price&quot;,L1:L2)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DAVERAGE(A1:C4,&quot;Price&quot;,L1:L2)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DSUM(A1:C4,&quot;Price&quot;,E1:F10)</f><v>6</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_takes_a_median() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>8</v></c><c r="C1"><f>MEDIAN(1,9,3)</f><v>0</v></c><c r="D1"><f>MEDIAN(1,2,3,4)</f><v>0</v></c><c r="E1"><f>MEDIAN(A1:B1)</f><v>0</v></c><c r="F1"><f>MEDIAN()</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>MEDIAN(1,9,3)</f><v>3</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>MEDIAN(1,2,3,4)</f><v>2.5</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>MEDIAN(A1:B1)</f><v>5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MEDIAN()</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_classifies_number_and_text() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>ISNUMBER(4)</f><v>0</v></c><c r="C1"><f>ISNUMBER("ab")</f><v>0</v></c><c r="D1"><f>ISTEXT("ab")</f><v>0</v></c><c r="E1"><f>ISTEXT(4)</f><v>0</v></c><c r="F1"><f>ISNUMBER(Z9)</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>ISNUMBER(4)</f><v>1</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>ISNUMBER("ab")</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>ISTEXT("ab")</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ISTEXT(4)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ISNUMBER(Z9)</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_gcd_and_lcm() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>GCD(12,18)</f><v>0</v></c><c r="C1"><f>GCD(12.9,18)</f><v>0</v></c><c r="D1"><f>LCM(4,6)</f><v>0</v></c><c r="E1"><f>LCM(0,5)</f><v>0</v></c><c r="F1"><f>GCD(-2,4)</f><v>7</v></c><c r="G1"><f>GCD()</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>GCD(12,18)</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GCD(12.9,18)</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>LCM(4,6)</f><v>12</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>LCM(0,5)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GCD(-2,4)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GCD()</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_logs_and_exp() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>LN(1)</f><v>0</v></c><c r="C1"><f>LOG10(100)</f><v>0</v></c><c r="D1"><f>LOG(100)</f><v>0</v></c><c r="E1"><f>LOG(8,2)</f><v>0</v></c><c r="F1"><f>EXP(0)</f><v>0</v></c><c r="G1"><f>EXP(1)</f><v>0</v></c><c r="H1"><f>LN(-1)</f><v>7</v></c><c r="I1"><f>LOG(8,1)</f><v>8</v></c><c r="J1"><f>EXP(1000)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>LN(1)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>LOG10(100)</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>LOG(100)</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>LOG(8,2)</f><v>3</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>EXP(0)</f><v>1</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>EXP(1)</f><v>2.71828183</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>LN(-1)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>LOG(8,1)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>EXP(1000)</f><v>9</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_factorial() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>FACT(5)</f><v>0</v></c><c r="C1"><f>FACT(0)</f><v>0</v></c><c r="D1"><f>FACT(5.9)</f><v>0</v></c><c r="E1"><f>FACT(-1)</f><v>7</v></c><c r="F1"><f>FACT(171)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>FACT(5)</f><v>120</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>FACT(0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>FACT(5.9)</f><v>120</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>FACT(-1)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>FACT(171)</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_trig_in_radians() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>SIN(0)</f><v>0</v></c><c r="C1"><f>COS(0)</f><v>0</v></c><c r="D1"><f>TAN(0)</f><v>0</v></c><c r="E1"><f>COS(PI())</f><v>0</v></c><c r="F1"><f>SIN(RADIANS(90))</f><v>0</v></c><c r="G1"><f>DEGREES(PI())</f><v>0</v></c><c r="H1"><f>RADIANS(180)</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>SIN(0)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COS(0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>TAN(0)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COS(PI())</f><v>-1</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>SIN(RADIANS(90))</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DEGREES(PI())</f><v>180</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RADIANS(180)</f><v>3.14159265</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_inverse_trig() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>ASIN(0)</f><v>0</v></c><c r="C1"><f>ACOS(0)</f><v>0</v></c><c r="D1"><f>ATAN(0)</f><v>0</v></c><c r="E1"><f>ATAN(1)</f><v>0</v></c><c r="F1"><f>ATAN2(1,0)</f><v>0</v></c><c r="G1"><f>ATAN2(0,1)</f><v>0</v></c><c r="H1"><f>ASIN(2)</f><v>7</v></c><c r="I1"><f>ATAN2(0,0)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>ASIN(0)</f><v>0</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>ACOS(0)</f><v>1.57079633</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>ATAN(0)</f><v>0</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>ATAN(1)</f><v>0.78539816</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>ATAN2(1,0)</f><v>0</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>ATAN2(0,1)</f><v>1.57079633</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>ASIN(2)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ATAN2(0,0)</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_hyperbolic() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>SINH(0)</f><v>0</v></c><c r="C1"><f>COSH(0)</f><v>0</v></c><c r="D1"><f>TANH(0)</f><v>0</v></c><c r="E1"><f>SINH(1)</f><v>0</v></c><c r="F1"><f>COSH(1000)</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>SINH(0)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COSH(0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>TANH(0)</f><v>0</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>SINH(1)</f><v>1.17520119</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>COSH(1000)</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_combin() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>COMBIN(5,2)</f><v>0</v></c><c r="C1"><f>COMBIN(5,0)</f><v>0</v></c><c r="D1"><f>COMBIN(5,5)</f><v>0</v></c><c r="E1"><f>COMBIN(5.9,2.2)</f><v>0</v></c><c r="F1"><f>COMBIN(4,5)</f><v>7</v></c><c r="G1"><f>COMBIN(-1,1)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>COMBIN(5,2)</f><v>10</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COMBIN(5,0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COMBIN(5,5)</f><v>1</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>COMBIN(5.9,2.2)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>COMBIN(4,5)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COMBIN(-1,1)</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_permutations() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>PERMUT(5,2)</f><v>0</v></c><c r="C1"><f>PERMUT(5,0)</f><v>0</v></c><c r="D1"><f>PERMUT(5,5)</f><v>0</v></c><c r="E1"><f>PERMUT(4,5)</f><v>7</v></c><c r="F1"><f>PERMUTATIONA(3,2)</f><v>0</v></c><c r="G1"><f>PERMUTATIONA(0,0)</f><v>0</v></c><c r="H1"><f>PERMUTATIONA(0,2)</f><v>0</v></c><c r="I1"><f>PERMUT(-1,1)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>PERMUT(5,2)</f><v>20</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>PERMUT(5,0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>PERMUT(5,5)</f><v>120</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>PERMUT(4,5)</f><v>7</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>PERMUTATIONA(3,2)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERMUTATIONA(0,0)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERMUTATIONA(0,2)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>PERMUT(-1,1)</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_large_and_small() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>8</v></c><c r="C1"><f>LARGE(1,9,3,1)</f><v>0</v></c><c r="D1"><f>LARGE(1,9,3,2)</f><v>0</v></c><c r="E1"><f>SMALL(1,9,3,1)</f><v>0</v></c><c r="F1"><f>SMALL(1,9,3,2.9)</f><v>0</v></c><c r="G1"><f>LARGE(A1:B1,1)</f><v>0</v></c><c r="H1"><f>LARGE(1,9,0)</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>LARGE(1,9,3,1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LARGE(1,9,3,2)</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SMALL(1,9,3,1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SMALL(1,9,3,2.9)</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LARGE(A1:B1,1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>LARGE(1,9,0)</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_truncates() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>TRUNC(1.239)</f><v>0</v></c><c r="C1"><f>TRUNC(1.239,2)</f><v>0</v></c><c r="D1"><f>TRUNC(-1.239,2)</f><v>0</v></c><c r="E1"><f>TRUNC(128,-1)</f><v>0</v></c><c r="F1"><f>TRUNC(1.2,20)</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>TRUNC(1.239)</f><v>1</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>TRUNC(1.239,2)</f><v>1.23</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TRUNC(-1.239,2)</f><v>-1.23</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TRUNC(128,-1)</f><v>120</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>TRUNC(1.2,20)</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_even_and_odd() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>ISEVEN(2.9)</f><v>0</v></c><c r="C1"><f>ISODD(2.9)</f><v>0</v></c><c r="D1"><f>ISEVEN(0)</f><v>0</v></c><c r="E1"><f>ISODD(-3)</f><v>0</v></c><c r="F1"><f>ISEVEN(-3.2)</f><v>0</v></c><c r="G1"><f>ISEVEN(1000000000000000)</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>ISEVEN(2.9)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ISODD(2.9)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ISEVEN(0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ISODD(-3)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ISEVEN(-3.2)</f><v>0</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>ISEVEN(1000000000000000)</f><v>7</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_code_and_char() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>CODE("A")</f><v>0</v></c><c r="C1"><f>CODE("Ab")</f><v>0</v></c><c r="D1"><f>CODE("")</f><v>7</v></c><c r="E1"><f>CHAR(65)</f><v>0</v></c><c r="F1"><f>CHAR(65.9)</f><v>0</v></c><c r="G1"><f>CHAR(8364)</f><v>0</v></c><c r="H1"><f>CHAR(0)</f><v>8</v></c><c r="I1"><f>CODE(A1)</f><v>0</v></c><c r="J1"><f>CHAR(55296)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>CODE("A")</f><v>65</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>CODE("Ab")</f><v>65</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>CODE("")</f><v>7</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>CHAR(65)</f><is><t>A</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHAR(65.9)</f><is><t>A</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains("<f>CHAR(8364)</f><is><t>\u{20AC}</t></is>"),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>CHAR(0)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>CODE(A1)</f><v>50</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>CHAR(55296)</f><v>9</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_counts_text_and_blanks() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="C1" t="inlineStr"><is><t>ab</t></is></c><c r="D1"><f>COUNTA(A1:C1)</f><v>0</v></c><c r="E1"><f>COUNTBLANK(A1:C1)</f><v>0</v></c><c r="F1"><f>COUNTA(1,"ab","")</f><v>0</v></c><c r="G1"><f>COUNTBLANK("")</f><v>0</v></c><c r="H1"><f>COUNTBLANK(4)</f><v>0</v></c><c r="I1"><f>COUNTA()</f><v>0</v></c><c r="J1"><f>COUNTA(Z9)</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>COUNTA(A1:C1)</f><v>2</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>COUNTBLANK(A1:C1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTA(1,"ab","")</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTBLANK("")</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>COUNTBLANK(4)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COUNTA()</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COUNTA(Z9)</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_iferror() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>IFERROR(SQRT(4),0)</f><v>0</v></c><c r="C1"><f>IFERROR(SQRT(-1),9)</f><v>0</v></c><c r="D1"><f>IFERROR(1/0,5)</f><v>0</v></c><c r="E1"><f>IFERROR(SQRT(-1),"no")</f><v>0</v></c><c r="F1"><f>IFERROR(SQRT(-1),SQRT(-1))</f><v>7</v></c><c r="G1"><f>IFERROR(1,2,3)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>IFERROR(SQRT(4),0)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>IFERROR(SQRT(-1),9)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>IFERROR(1/0,5)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>IFERROR(SQRT(-1),"no")</f><is><t>no</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>IFERROR(SQRT(-1),SQRT(-1))</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>IFERROR(1,2,3)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_clean_and_proper() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                "<worksheet><sheetData><row r=\"1\"><c r=\"A1\"><v>1</v></c><c r=\"B1\"><f>CLEAN(CHAR(10)&amp;\"ab\")</f><v>0</v></c><c r=\"C1\"><f>CLEAN(\"ab\")</f><v>0</v></c><c r=\"D1\"><f>PROPER(\"ab cd\")</f><v>0</v></c><c r=\"E1\"><f>PROPER(\"a1b\")</f><v>0</v></c><c r=\"F1\"><f>PROPER(\"\u{00C9}RIC\")</f><v>0</v></c></row></sheetData></worksheet>",
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>CLEAN(CHAR(10)&amp;"ab")</f><is><t>ab</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CLEAN("ab")</f><is><t>ab</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PROPER("ab cd")</f><is><t>Ab Cd</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PROPER("a1b")</f><is><t>A1B</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains("<f>PROPER(\"\u{00C9}RIC\")</f><is><t>\u{00C9}ric</t></is>"),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_choose() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>CHOOSE(2,10,20,30)</f><v>0</v></c><c r="C1"><f>CHOOSE(2.9,"a","b")</f><v>0</v></c><c r="D1"><f>CHOOSE(1,SQRT(-1),5)</f><v>7</v></c><c r="E1"><f>CHOOSE(0,1,2)</f><v>8</v></c><c r="F1"><f>CHOOSE(3,1,2)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>CHOOSE(2,10,20,30)</f><v>20</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHOOSE(2.9,"a","b")</f><is><t>b</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHOOSE(1,SQRT(-1),5)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>CHOOSE(0,1,2)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>CHOOSE(3,1,2)</f><v>9</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_switch() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>SWITCH(2,1,10,2,20)</f><v>0</v></c><c r="C1"><f>SWITCH(2,1,10,9)</f><v>0</v></c><c r="D1"><f>SWITCH(3,1,10)</f><v>7</v></c><c r="E1"><f>SWITCH("b","a",1,"b",2)</f><v>0</v></c><c r="F1"><f>SWITCH(1,1,SQRT(-1))</f><v>8</v></c><c r="G1"><f>SWITCH(1,"1",9)</f><v>6</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SWITCH(2,1,10,2,20)</f><v>20</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SWITCH(2,1,10,9)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SWITCH(3,1,10)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SWITCH("b","a",1,"b",2)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SWITCH(1,1,SQRT(-1))</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SWITCH(1,"1",9)</f><v>6</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_xor() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>XOR(1,0)</f><v>0</v></c><c r="C1"><f>XOR(1,0,1)</f><v>0</v></c><c r="D1"><f>XOR(0,0)</f><v>0</v></c><c r="E1"><f>XOR(1,1,1)</f><v>0</v></c><c r="F1"><f>XOR()</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>XOR(1,0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>XOR(1,0,1)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>XOR(0,0)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>XOR(1,1,1)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>XOR()</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_text_join() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="C1" t="inlineStr"><is><t>ab</t></is></c><c r="D1"><f>TEXTJOIN(",",1,"a","","b")</f><v>0</v></c><c r="E1"><f>TEXTJOIN(",",0,"a","","b")</f><v>0</v></c><c r="F1"><f>TEXTJOIN(",",1,A1:C1)</f><v>0</v></c><c r="G1"><f>TEXTJOIN(",",0,A1:C1)</f><v>0</v></c><c r="H1"><f>TEXTJOIN(",",1)</f><v>0</v></c><c r="I1"><f>TEXTJOIN("x",0,REPT("a",32767),"b")</f><v>7</v></c><c r="J1"><f>TEXTJOIN(",",1,Z9)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>TEXTJOIN(",",1,"a","","b")</f><is><t>a,b</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXTJOIN(",",0,"a","","b")</f><is><t>a,,b</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXTJOIN(",",1,A1:C1)</f><is><t>2,ab</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXTJOIN(",",0,A1:C1)</f><is><t>2,,ab</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXTJOIN(",",1)</f><is><t></t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXTJOIN("x",0,REPT("a",32767),"b")</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXTJOIN(",",1,Z9)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_ifs() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>IFS(0,1,1,9)</f><v>0</v></c><c r="C1"><f>IFS(0,1)</f><v>7</v></c><c r="D1"><f>IFS(0,SQRT(-1),1,5)</f><v>0</v></c><c r="E1"><f>IFS(1,SQRT(-1),1,5)</f><v>8</v></c><c r="F1"><f>IFS("a",1)</f><v>6</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>IFS(0,1,1,9)</f><v>9</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>IFS(0,1)</f><v>7</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>IFS(0,SQRT(-1),1,5)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>IFS(1,SQRT(-1),1,5)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>IFS("a",1)</f><v>6</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_if_text() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>IF(1,&quot;cat&quot;,2)</f><v>0</v></c><c r="C1"><f>IF(0,&quot;cat&quot;,9)</f><v>0</v></c><c r="D1"><f>IFS(1,&quot;pear&quot;,0,2)</f><v>0</v></c><c r="E1"><f>IF(1,SQRT(-1),&quot;no&quot;)</f><v>4</v></c><c r="F1"><f>IF(0,&quot;no&quot;,SQRT(-1))</f><v>5</v></c><c r="G1"><f>IFS(0,&quot;no&quot;,1,&quot;yes&quot;)</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(
                r#"<c r="B1" t="inlineStr"><f>IF(1,&quot;cat&quot;,2)</f><is><t>cat</t></is></c>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>IF(0,&quot;cat&quot;,9)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="D1" t="inlineStr"><f>IFS(1,&quot;pear&quot;,0,2)</f><is><t>pear</t></is></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>IF(1,SQRT(-1),&quot;no&quot;)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>IF(0,&quot;no&quot;,SQRT(-1))</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="G1" t="inlineStr"><f>IFS(0,&quot;no&quot;,1,&quot;yes&quot;)</f><is><t>yes</t></is></c>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_bitwise() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>BITAND(13,25)</f><v>0</v></c><c r="C1"><f>BITOR(13,25)</f><v>0</v></c><c r="D1"><f>BITXOR(13,25)</f><v>0</v></c><c r="E1"><f>BITAND(1.9,1)</f><v>0</v></c><c r="F1"><f>BITAND(-1,1)</f><v>7</v></c><c r="G1"><f>BITAND(281474976710656,1)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>BITAND(13,25)</f><v>9</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>BITOR(13,25)</f><v>29</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>BITXOR(13,25)</f><v>20</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>BITAND(1.9,1)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>BITAND(-1,1)</f><v>7</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>BITAND(281474976710656,1)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_ceiling_and_floor() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>CEILING(2.5,1)</f><v>0</v></c><c r="C1"><f>CEILING(-2.5,-1)</f><v>0</v></c><c r="D1"><f>CEILING(-2.5,1)</f><v>7</v></c><c r="E1"><f>CEILING(4,0)</f><v>0</v></c><c r="F1"><f>FLOOR(2.5,1)</f><v>0</v></c><c r="G1"><f>FLOOR(-2.5,-1)</f><v>0</v></c><c r="H1"><f>FLOOR(4,0)</f><v>8</v></c><c r="I1"><f>CEILING(2.5)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>CEILING(2.5,1)</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CEILING(-2.5,-1)</f><v>-3</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CEILING(-2.5,1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>CEILING(4,0)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>FLOOR(2.5,1)</f><v>2</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>FLOOR(-2.5,-1)</f><v>-2</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>FLOOR(4,0)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>CEILING(2.5)</f><v>9</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_bit_shift() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>BITLSHIFT(5,2)</f><v>0</v></c><c r="C1"><f>BITRSHIFT(20,2)</f><v>0</v></c><c r="D1"><f>BITLSHIFT(5,-1)</f><v>0</v></c><c r="E1"><f>BITRSHIFT(5,-1)</f><v>0</v></c><c r="F1"><f>BITLSHIFT(1,48)</f><v>7</v></c><c r="G1"><f>BITLSHIFT(1,54)</f><v>8</v></c><c r="H1"><f>BITLSHIFT(-1,1)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>BITLSHIFT(5,2)</f><v>20</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BITRSHIFT(20,2)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BITLSHIFT(5,-1)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BITRSHIFT(5,-1)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BITLSHIFT(1,48)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BITLSHIFT(1,54)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BITLSHIFT(-1,1)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_mround() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>MROUND(10,3)</f><v>0</v></c><c r="C1"><f>MROUND(10,4)</f><v>0</v></c><c r="D1"><f>MROUND(-10,-3)</f><v>0</v></c><c r="E1"><f>MROUND(-10,3)</f><v>7</v></c><c r="F1"><f>MROUND(6,0)</f><v>8</v></c><c r="G1"><f>MROUND(0,0)</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>MROUND(10,3)</f><v>9</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MROUND(10,4)</f><v>12</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>MROUND(-10,-3)</f><v>-9</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>MROUND(-10,3)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MROUND(6,0)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MROUND(0,0)</f><v>0</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_sumif() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>8</v></c><c r="C1"><v>3</v></c><c r="D1"><f>SUMIF(A1:C1,"&gt;2")</f><v>0</v></c><c r="E1"><f>SUMIF(A1:C1,8)</f><v>0</v></c><c r="F1"><f>SUMIF(A1:C1,"&lt;&gt;8")</f><v>0</v></c><c r="G1"><f>SUMIF(A1:C1,"&lt;0")</f><v>0</v></c><c r="H1"><f>SUMIF(A1:C1,"ab")</f><v>7</v></c><c r="I1"><f>SUMIF(A1:C1,"&gt;2",A1)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SUMIF(A1:C1,"&gt;2")</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMIF(A1:C1,8)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMIF(A1:C1,"&lt;&gt;8")</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMIF(A1:C1,"&lt;0")</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMIF(A1:C1,"ab")</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMIF(A1:C1,"&gt;2",A1)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_countif() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>8</v></c><c r="C1"><v>3</v></c><c r="D1"><f>COUNTIF(A1:C1,"&gt;2")</f><v>0</v></c><c r="E1"><f>COUNTIF(A1:C1,8)</f><v>0</v></c><c r="F1"><f>COUNTIF(A1:C1,"&lt;&gt;8")</f><v>0</v></c><c r="G1"><f>COUNTIF(A1:C1,"&lt;0")</f><v>0</v></c><c r="H1"><f>COUNTIF(A1:C1,"ab")</f><v>7</v></c><c r="I1"><f>COUNTIF(A1:C1,"&gt;2",A1)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>COUNTIF(A1:C1,"&gt;2")</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTIF(A1:C1,8)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTIF(A1:C1,"&lt;&gt;8")</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTIF(A1:C1,"&lt;0")</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTIF(A1:C1,"ab")</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTIF(A1:C1,"&gt;2",A1)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_countifs() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>8</v></c><c r="C1"><v>3</v></c><c r="D1"><f>COUNTIFS(A1:C1,"&gt;2")</f><v>0</v></c><c r="E1"><f>COUNTIFS(A1:C1,8)</f><v>0</v></c><c r="F1"><f>COUNTIFS(A1:C1,"&lt;&gt;8")</f><v>0</v></c><c r="G1"><f>COUNTIFS(A1:C1,"&lt;0")</f><v>0</v></c><c r="H1"><f>COUNTIFS(A1:C1,"ab")</f><v>7</v></c><c r="I1"><f>COUNTIFS(A1:C1,"&gt;2",B1:B1,1)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>COUNTIFS(A1:C1,"&gt;2")</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTIFS(A1:C1,8)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTIFS(A1:C1,"&lt;&gt;8")</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTIFS(A1:C1,"&lt;0")</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTIFS(A1:C1,"ab")</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTIFS(A1:C1,"&gt;2",B1:B1,1)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_slope() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>4</v></c><c r="C1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><v>1</v></c><c r="E1"><v>2</v></c><c r="F1"><v>3</v></c><c r="I1"><v>5</v></c><c r="J1"><v>5</v></c><c r="G1"><f>SLOPE(A1:C1,D1:F1)</f><v>0</v></c><c r="H1"><f>SLOPE(A1:B1,D1:F1)</f><v>7</v></c><c r="K1"><f>SLOPE(A1:B1,I1:J1)</f><v>8</v></c><c r="L1"><f>SLOPE(A1:A1,D1:D1)</f><v>9</v></c><c r="M1"><f>SLOPE(1,2)</f><v>10</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SLOPE(A1:C1,D1:F1)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SLOPE(A1:B1,D1:F1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SLOPE(A1:B1,I1:J1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SLOPE(A1:A1,D1:D1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>SLOPE(1,2)</f><v>10</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_intercept() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>4</v></c><c r="C1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><v>1</v></c><c r="E1"><v>2</v></c><c r="F1"><v>3</v></c><c r="I1"><v>5</v></c><c r="J1"><v>5</v></c><c r="G1"><f>INTERCEPT(A1:C1,D1:F1)</f><v>0</v></c><c r="H1"><f>INTERCEPT(A1:B1,D1:F1)</f><v>7</v></c><c r="K1"><f>INTERCEPT(A1:B1,I1:J1)</f><v>8</v></c><c r="L1"><f>INTERCEPT(A1:A1,D1:D1)</f><v>9</v></c><c r="M1"><f>INTERCEPT(1,2)</f><v>10</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>INTERCEPT(A1:C1,D1:F1)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>INTERCEPT(A1:B1,D1:F1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>INTERCEPT(A1:B1,I1:J1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>INTERCEPT(A1:A1,D1:D1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>INTERCEPT(1,2)</f><v>10</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_correl() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>4</v></c><c r="C1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><v>1</v></c><c r="E1"><v>2</v></c><c r="F1"><v>3</v></c><c r="I1"><v>5</v></c><c r="J1"><v>5</v></c><c r="N1"><v>3</v></c><c r="O1"><v>3</v></c><c r="P1"><v>1</v></c><c r="Q1"><v>2</v></c><c r="G1"><f>CORREL(A1:C1,D1:F1)</f><v>0</v></c><c r="H1"><f>PEARSON(A1:C1,D1:F1)</f><v>0</v></c><c r="K1"><f>CORREL(A1:B1,D1:F1)</f><v>7</v></c><c r="L1"><f>CORREL(A1:B1,I1:J1)</f><v>8</v></c><c r="M1"><f>CORREL(N1:O1,P1:Q1)</f><v>11</v></c><c r="R1"><f>CORREL(A1:A1,D1:D1)</f><v>9</v></c><c r="S1"><f>CORREL(1,2)</f><v>10</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>CORREL(A1:C1,D1:F1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PEARSON(A1:C1,D1:F1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CORREL(A1:B1,D1:F1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CORREL(A1:B1,I1:J1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CORREL(N1:O1,P1:Q1)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CORREL(A1:A1,D1:D1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>CORREL(1,2)</f><v>10</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_rsq() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>4</v></c><c r="C1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><v>1</v></c><c r="E1"><v>2</v></c><c r="F1"><v>3</v></c><c r="I1"><v>5</v></c><c r="J1"><v>5</v></c><c r="N1"><v>4</v></c><c r="O1"><v>2</v></c><c r="P1"><v>1</v></c><c r="Q1"><v>2</v></c><c r="G1"><f>RSQ(A1:C1,D1:F1)</f><v>0</v></c><c r="H1"><f>RSQ(N1:O1,P1:Q1)</f><v>0</v></c><c r="K1"><f>RSQ(A1:B1,D1:F1)</f><v>7</v></c><c r="L1"><f>RSQ(A1:B1,I1:J1)</f><v>8</v></c><c r="R1"><f>RSQ(A1:A1,D1:D1)</f><v>9</v></c><c r="S1"><f>RSQ(1,2)</f><v>10</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>RSQ(A1:C1,D1:F1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RSQ(N1:O1,P1:Q1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RSQ(A1:B1,D1:F1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RSQ(A1:B1,I1:J1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RSQ(A1:A1,D1:D1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>RSQ(1,2)</f><v>10</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_forecast() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>4</v></c><c r="C1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><v>1</v></c><c r="E1"><v>2</v></c><c r="F1"><v>3</v></c><c r="I1"><v>5</v></c><c r="J1"><v>5</v></c><c r="G1"><f>FORECAST(4,A1:C1,D1:F1)</f><v>0</v></c><c r="H1"><f>FORECAST.LINEAR(0,A1:C1,D1:F1)</f><v>0</v></c><c r="K1"><f>FORECAST(4,A1:B1,D1:F1)</f><v>7</v></c><c r="L1"><f>FORECAST(4,A1:B1,I1:J1)</f><v>8</v></c><c r="M1"><f>FORECAST(4,A1:A1,D1:D1)</f><v>9</v></c><c r="N1"><f>FORECAST("ab",A1:C1,D1:F1)</f><v>10</v></c><c r="O1"><f>FORECAST(4,1,2)</f><v>11</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>FORECAST(4,A1:C1,D1:F1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FORECAST.LINEAR(0,A1:C1,D1:F1)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FORECAST(4,A1:B1,D1:F1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FORECAST(4,A1:B1,I1:J1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FORECAST(4,A1:A1,D1:D1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FORECAST("ab",A1:C1,D1:F1)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FORECAST(4,1,2)</f><v>11</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_steyx() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>2</v></c><c r="C1"><v>4</v></c><c r="D1"><v>1</v></c><c r="E1"><v>2</v></c><c r="F1"><v>3</v></c><c r="I1"><v>5</v></c><c r="J1"><v>5</v></c><c r="K1"><v>5</v></c><c r="N1"><v>2</v></c><c r="O1"><v>4</v></c><c r="P1"><v>6</v></c><c r="Q1"><v>1</v></c><c r="R1"><v>2</v></c><c r="S1"><v>3</v></c><c r="G1"><f>STEYX(A1:C1,D1:F1)</f><v>0</v></c><c r="H1"><f>STEYX(N1:P1,Q1:S1)</f><v>0</v></c><c r="L1"><f>STEYX(A1:B1,D1:E1)</f><v>7</v></c><c r="M1"><f>STEYX(A1:B1,D1:F1)</f><v>8</v></c><c r="T1"><f>STEYX(A1:C1,I1:K1)</f><v>9</v></c><c r="U1"><f>STEYX(1,2)</f><v>10</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>STEYX(A1:C1,D1:F1)</f><v>0.40824829</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>STEYX(N1:P1,Q1:S1)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>STEYX(A1:B1,D1:E1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>STEYX(A1:B1,D1:F1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>STEYX(A1:C1,I1:K1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>STEYX(1,2)</f><v>10</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_covariance() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>4</v></c><c r="C1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><v>1</v></c><c r="E1"><v>2</v></c><c r="F1"><v>3</v></c><c r="I1"><v>5</v></c><c r="J1"><v>5</v></c><c r="G1"><f>COVARIANCE.P(A1:C1,D1:F1)</f><v>0</v></c><c r="H1"><f>COVARIANCE.S(A1:C1,D1:F1)</f><v>0</v></c><c r="K1"><f>COVAR(A1:C1,D1:F1)</f><v>0</v></c><c r="L1"><f>COVARIANCE.P(A1:B1,D1:F1)</f><v>7</v></c><c r="M1"><f>COVARIANCE.P(A1:B1,I1:J1)</f><v>9</v></c><c r="N1"><f>COVARIANCE.P(A1:A1,D1:D1)</f><v>8</v></c><c r="O1"><f>COVARIANCE.P(1,2)</f><v>10</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>COVARIANCE.P(A1:C1,D1:F1)</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COVARIANCE.S(A1:C1,D1:F1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COVAR(A1:C1,D1:F1)</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COVARIANCE.P(A1:B1,D1:F1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COVARIANCE.P(A1:B1,I1:J1)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COVARIANCE.P(A1:A1,D1:D1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COVARIANCE.P(1,2)</f><v>10</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_rank() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>20</v></c><c r="C1"><v>20</v></c><c r="D1"><v>30</v></c><c r="E1" t="inlineStr"><is><t>xy</t></is></c><c r="F1"><f>RANK(D1,A1:E1)</f><v>0</v></c><c r="G1"><f>RANK.EQ(B1,A1:E1)</f><v>0</v></c><c r="H1"><f>RANK(A1,A1:E1)</f><v>0</v></c><c r="I1"><f>RANK(B1,A1:E1,1)</f><v>0</v></c><c r="J1"><f>RANK(15,A1:E1)</f><v>7</v></c><c r="K1"><f>RANK(B1,A1)</f><v>8</v></c><c r="L1"><f>RANK(B1,A1:E1,1,2)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "10").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>RANK(D1,A1:E1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RANK.EQ(B1,A1:E1)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RANK(A1,A1:E1)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RANK(B1,A1:E1,1)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RANK(15,A1:E1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>RANK(B1,A1)</f><v>8</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>RANK(B1,A1:E1,1,2)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_rank_avg() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>20</v></c><c r="C1"><v>20</v></c><c r="D1"><v>30</v></c><c r="E1" t="inlineStr"><is><t>xy</t></is></c><c r="F1"><f>RANK.AVG(B1,A1:E1)</f><v>0</v></c><c r="G1"><f>RANK.AVG(D1,A1:E1)</f><v>0</v></c><c r="H1"><f>RANK.AVG(A1,A1:E1)</f><v>0</v></c><c r="I1"><f>RANK.AVG(B1,A1:E1,1)</f><v>0</v></c><c r="J1"><f>RANK.AVG(15,A1:E1)</f><v>7</v></c><c r="K1"><f>RANK.AVG(B1,A1)</f><v>8</v></c><c r="L1"><f>RANK.AVG(B1,A1:E1,1,2)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "10").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>RANK.AVG(B1,A1:E1)</f><v>2.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RANK.AVG(D1,A1:E1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RANK.AVG(A1,A1:E1)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RANK.AVG(B1,A1:E1,1)</f><v>2.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RANK.AVG(15,A1:E1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RANK.AVG(B1,A1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RANK.AVG(B1,A1:E1,1,2)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_percentile() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>20</v></c><c r="C1"><v>30</v></c><c r="D1"><v>40</v></c><c r="E1" t="inlineStr"><is><t>xy</t></is></c><c r="F1"><f>PERCENTILE(A1:E1,0)</f><v>0</v></c><c r="G1"><f>PERCENTILE(A1:E1,1)</f><v>0</v></c><c r="H1"><f>PERCENTILE.INC(A1:E1,0.25)</f><v>0</v></c><c r="I1"><f>PERCENTILE(A1:E1,0.5)</f><v>0</v></c><c r="J1"><f>PERCENTILE(A1:E1,-0.1)</f><v>7</v></c><c r="K1"><f>PERCENTILE(A1:E1,2)</f><v>8</v></c><c r="L1"><f>PERCENTILE(A1,0)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "10").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>PERCENTILE(A1:E1,0)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE(A1:E1,1)</f><v>40</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE.INC(A1:E1,0.25)</f><v>17.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE(A1:E1,0.5)</f><v>25</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE(A1:E1,-0.1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE(A1:E1,2)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE(A1,0)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_quartile() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>20</v></c><c r="C1"><v>30</v></c><c r="D1"><v>40</v></c><c r="E1" t="inlineStr"><is><t>xy</t></is></c><c r="F1"><f>QUARTILE(A1:E1,0)</f><v>0</v></c><c r="G1"><f>QUARTILE(A1:E1,1)</f><v>0</v></c><c r="H1"><f>QUARTILE.INC(A1:E1,1.9)</f><v>0</v></c><c r="I1"><f>QUARTILE(A1:E1,2)</f><v>0</v></c><c r="J1"><f>QUARTILE(A1:E1,3)</f><v>0</v></c><c r="K1"><f>QUARTILE(A1:E1,4)</f><v>0</v></c><c r="L1"><f>QUARTILE(A1:E1,5)</f><v>7</v></c><c r="M1"><f>QUARTILE(A1:E1,-1)</f><v>8</v></c><c r="N1"><f>QUARTILE(A1,0)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "10").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>QUARTILE(A1:E1,0)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE(A1:E1,1)</f><v>17.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE.INC(A1:E1,1.9)</f><v>17.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE(A1:E1,2)</f><v>25</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE(A1:E1,3)</f><v>32.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE(A1:E1,4)</f><v>40</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE(A1:E1,5)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE(A1:E1,-1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE(A1,0)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_mode() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>20</v></c><c r="C1"><v>20</v></c><c r="D1"><v>10</v></c><c r="E1" t="inlineStr"><is><t>xy</t></is></c><c r="F1"><v>30</v></c><c r="G1"><v>30</v></c><c r="H1"><v>30</v></c><c r="J1"><f>MODE(A1:H1)</f><v>0</v></c><c r="K1"><f>MODE.SNGL(A1:D1)</f><v>0</v></c><c r="L1"><f>MODE(A1:A1)</f><v>7</v></c><c r="M1"><f>MODE(A1)</f><v>8</v></c><c r="N1"><f>MODE(A1:D1,1)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "10").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>MODE(A1:H1)</f><v>30</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>MODE.SNGL(A1:D1)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>MODE(A1:A1)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MODE(A1)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MODE(A1:D1,1)</f><v>9</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_percent_rank() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>20</v></c><c r="C1"><v>30</v></c><c r="D1"><v>40</v></c><c r="E1" t="inlineStr"><is><t>xy</t></is></c><c r="F1"><f>PERCENTRANK(A1:E1,A1)</f><v>0</v></c><c r="G1"><f>PERCENTRANK(A1:E1,D1)</f><v>0</v></c><c r="H1"><f>PERCENTRANK.INC(A1:E1,B1)</f><v>0</v></c><c r="I1"><f>PERCENTRANK(A1:E1,25)</f><v>0</v></c><c r="J1"><f>PERCENTRANK(A1:E1,5)</f><v>7</v></c><c r="K1"><f>PERCENTRANK(A1:E1,50)</f><v>8</v></c><c r="L1"><f>PERCENTRANK(A1,10)</f><v>9</v></c><c r="M1"><f>PERCENTRANK(A1:E1,20,3)</f><v>11</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "10").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>PERCENTRANK(A1:E1,A1)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK(A1:E1,D1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK.INC(A1:E1,B1)</f><v>0.33333333</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK(A1:E1,25)</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK(A1:E1,5)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK(A1:E1,50)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK(A1,10)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK(A1:E1,20,3)</f><v>11</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_percent_rank_exc() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>20</v></c><c r="C1"><v>30</v></c><c r="D1"><v>40</v></c><c r="E1" t="inlineStr"><is><t>xy</t></is></c><c r="F1"><f>PERCENTRANK.EXC(A1:E1,A1)</f><v>0</v></c><c r="G1"><f>PERCENTRANK.EXC(A1:E1,D1)</f><v>0</v></c><c r="H1"><f>PERCENTRANK.EXC(A1:E1,B1)</f><v>0</v></c><c r="I1"><f>PERCENTRANK.EXC(A1:E1,25)</f><v>0</v></c><c r="J1"><f>PERCENTRANK.EXC(A1:E1,5)</f><v>7</v></c><c r="K1"><f>PERCENTRANK.EXC(A1:E1,50)</f><v>8</v></c><c r="L1"><f>PERCENTRANK.EXC(A1,10)</f><v>9</v></c><c r="M1"><f>PERCENTRANK.EXC(A1:E1,20,3)</f><v>11</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "10").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>PERCENTRANK.EXC(A1:E1,A1)</f><v>0.2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK.EXC(A1:E1,D1)</f><v>0.8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK.EXC(A1:E1,B1)</f><v>0.4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK.EXC(A1:E1,25)</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK.EXC(A1:E1,5)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK.EXC(A1:E1,50)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK.EXC(A1,10)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK.EXC(A1:E1,20,3)</f><v>11</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_standardize() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>40</v></c><c r="C1"><v>1.5</v></c><c r="D1"><v>0</v></c><c r="E1"><f>STANDARDIZE(A1,B1,C1)</f><v>0</v></c><c r="F1"><f>STANDARDIZE(A1,A1,5)</f><v>0</v></c><c r="G1"><f>STANDARDIZE(A1,B1,D1)</f><v>7</v></c><c r="H1"><f>STANDARDIZE(A1,B1,-2)</f><v>8</v></c><c r="I1"><f>STANDARDIZE(A1,B1)</f><v>9</v></c><c r="J1"><f>STANDARDIZE(&quot;ab&quot;,1,2)</f><v>11</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "42").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>STANDARDIZE(A1,B1,C1)</f><v>1.33333333</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>STANDARDIZE(A1,A1,5)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>STANDARDIZE(A1,B1,D1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>STANDARDIZE(A1,B1,-2)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>STANDARDIZE(A1,B1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>STANDARDIZE(&quot;ab&quot;,1,2)</f><v>11</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_skew() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>1</v></c><c r="C1"><v>4</v></c><c r="D1" t="inlineStr"><is><t>xy</t></is></c><c r="E1"><v>5</v></c><c r="F1"><v>5</v></c><c r="G1"><v>5</v></c><c r="H1"><f>SKEW(A1:D1)</f><v>0</v></c><c r="I1"><f>SKEW.P(A1:C1)</f><v>0</v></c><c r="J1"><f>SKEW(A1:B1)</f><v>5</v></c><c r="K1"><f>SKEW(E1:G1)</f><v>6</v></c><c r="L1"><f>SKEW()</f><v>7</v></c><c r="M1"><f>SKEW(&quot;ab&quot;)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SKEW(A1:D1)</f><v>1.73205081</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SKEW.P(A1:C1)</f><v>0.70710678</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>SKEW(A1:B1)</f><v>5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SKEW(E1:G1)</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SKEW()</f><v>7</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>SKEW(&quot;ab&quot;)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_kurt() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>1</v></c><c r="C1"><v>1</v></c><c r="D1"><v>3</v></c><c r="E1" t="inlineStr"><is><t>xy</t></is></c><c r="F1"><v>2</v></c><c r="G1"><v>2</v></c><c r="H1"><v>2</v></c><c r="I1"><v>2</v></c><c r="J1"><f>KURT(A1:E1)</f><v>0</v></c><c r="K1"><f>KURT(A1:C1)</f><v>5</v></c><c r="L1"><f>KURT(F1:I1)</f><v>6</v></c><c r="M1"><f>KURT()</f><v>7</v></c><c r="N1"><f>KURT(&quot;ab&quot;)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>KURT(A1:E1)</f><v>4</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>KURT(A1:C1)</f><v>5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>KURT(F1:I1)</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>KURT()</f><v>7</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>KURT(&quot;ab&quot;)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_trimmean() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>20</v></c><c r="C1"><v>30</v></c><c r="D1"><v>100</v></c><c r="E1" t="inlineStr"><is><t>xy</t></is></c><c r="F1"><f>TRIMMEAN(A1:E1,0.5)</f><v>0</v></c><c r="G1"><f>TRIMMEAN(A1:E1,0)</f><v>0</v></c><c r="H1"><f>TRIMMEAN(A1:E1,0.2)</f><v>0</v></c><c r="I1"><f>TRIMMEAN(A1:E1,1)</f><v>7</v></c><c r="J1"><f>TRIMMEAN(A1:E1,-0.1)</f><v>8</v></c><c r="K1"><f>TRIMMEAN(A1,0)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "10").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>TRIMMEAN(A1:E1,0.5)</f><v>25</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TRIMMEAN(A1:E1,0)</f><v>40</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TRIMMEAN(A1:E1,0.2)</f><v>40</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TRIMMEAN(A1:E1,1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TRIMMEAN(A1:E1,-0.1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TRIMMEAN(A1,0)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_fisher() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>FISHER(A1)</f><v>0</v></c><c r="C1"><f>FISHER(0)</f><v>0</v></c><c r="D1"><f>FISHERINV(0)</f><v>0</v></c><c r="E1"><f>FISHERINV(1)</f><v>0</v></c><c r="F1"><f>FISHER(1)</f><v>7</v></c><c r="G1"><f>FISHER(-1)</f><v>8</v></c><c r="H1"><f>FISHER(2)</f><v>9</v></c><c r="I1"><f>FISHER(&quot;ab&quot;)</f><v>11</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "0.5").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>FISHER(A1)</f><v>0.54930614</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>FISHER(0)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>FISHERINV(0)</f><v>0</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>FISHERINV(1)</f><v>0.76159416</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>FISHER(1)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>FISHER(-1)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>FISHER(2)</f><v>9</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>FISHER(&quot;ab&quot;)</f><v>11</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_sqrt_pi() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>SQRTPI(A1)</f><v>0</v></c><c r="C1"><f>SQRTPI(0)</f><v>0</v></c><c r="D1"><f>SQRTPI(-1)</f><v>7</v></c><c r="E1"><f>SQRTPI(&quot;ab&quot;)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SQRTPI(A1)</f><v>1.77245385</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>SQRTPI(0)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SQRTPI(-1)</f><v>7</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>SQRTPI(&quot;ab&quot;)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_combina() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>COMBINA(A1,3)</f><v>0</v></c><c r="C1"><f>COMBINA(4.9,3.2)</f><v>0</v></c><c r="D1"><f>COMBINA(4,0)</f><v>0</v></c><c r="E1"><f>COMBINA(0,0)</f><v>0</v></c><c r="F1"><f>COMBINA(0,2)</f><v>0</v></c><c r="G1"><f>COMBINA(-1,1)</f><v>7</v></c><c r="H1"><f>COMBINA(4,-1)</f><v>8</v></c><c r="I1"><f>COMBINA(&quot;ab&quot;,1)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "4").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>COMBINA(A1,3)</f><v>20</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COMBINA(4.9,3.2)</f><v>20</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>COMBINA(4,0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COMBINA(0,0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COMBINA(0,2)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COMBINA(-1,1)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COMBINA(4,-1)</f><v>8</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>COMBINA(&quot;ab&quot;,1)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_sum_x() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>3</v></c><c r="C1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><v>4</v></c><c r="E1"><v>1</v></c><c r="F1"><v>9</v></c><c r="G1"><f>SUMX2MY2(A1:C1,D1:F1)</f><v>0</v></c><c r="H1"><f>SUMX2PY2(A1:C1,D1:F1)</f><v>0</v></c><c r="I1"><f>SUMXMY2(A1:C1,D1:F1)</f><v>0</v></c><c r="J1"><f>SUMX2MY2(A1:B1,D1:F1)</f><v>7</v></c><c r="K1"><f>SUMX2MY2(1,2)</f><v>8</v></c><c r="L1"><v>1e200</v></c><c r="M1"><v>1</v></c><c r="N1"><f>SUMX2MY2(L1:L1,M1:M1)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SUMX2MY2(A1:C1,D1:F1)</f><v>-4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMX2PY2(A1:C1,D1:F1)</f><v>30</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMXMY2(A1:C1,D1:F1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMX2MY2(A1:B1,D1:F1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>SUMX2MY2(1,2)</f><v>8</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>SUMX2MY2(L1:L1,M1:M1)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_gestep() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>GESTEP(A1,4)</f><v>0</v></c><c r="C1"><f>GESTEP(A1,5)</f><v>0</v></c><c r="D1"><f>GESTEP(A1,6)</f><v>0</v></c><c r="E1"><f>GESTEP(A1)</f><v>0</v></c><c r="F1"><f>GESTEP(-1)</f><v>0</v></c><c r="G1"><f>DELTA(A1,5)</f><v>0</v></c><c r="H1"><f>DELTA(A1,4)</f><v>0</v></c><c r="I1"><f>DELTA(0)</f><v>0</v></c><c r="J1"><f>DELTA(A1)</f><v>0</v></c><c r="K1"><f>GESTEP(&quot;ab&quot;)</f><v>7</v></c><c r="L1"><f>DELTA(A1,&quot;ab&quot;)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "5").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>GESTEP(A1,4)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GESTEP(A1,5)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GESTEP(A1,6)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GESTEP(A1)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GESTEP(-1)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>DELTA(A1,5)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>DELTA(A1,4)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>DELTA(0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>DELTA(A1)</f><v>0</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>GESTEP(&quot;ab&quot;)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DELTA(A1,&quot;ab&quot;)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_multinomial() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>3</v></c><c r="C1"><v>4</v></c><c r="D1" t="inlineStr"><is><t>xy</t></is></c><c r="E1"><f>MULTINOMIAL(A1:D1)</f><v>0</v></c><c r="F1"><f>MULTINOMIAL(2.9,3.2)</f><v>0</v></c><c r="G1"><f>MULTINOMIAL(0)</f><v>0</v></c><c r="H1"><f>MULTINOMIAL()</f><v>7</v></c><c r="I1"><f>MULTINOMIAL(-1,2)</f><v>8</v></c><c r="J1"><f>MULTINOMIAL(&quot;ab&quot;)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>MULTINOMIAL(A1:D1)</f><v>1260</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MULTINOMIAL(2.9,3.2)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MULTINOMIAL(0)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>MULTINOMIAL()</f><v>7</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>MULTINOMIAL(-1,2)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MULTINOMIAL(&quot;ab&quot;)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_fact_double() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>FACTDOUBLE(A1)</f><v>0</v></c><c r="C1"><f>FACTDOUBLE(7.9)</f><v>0</v></c><c r="D1"><f>FACTDOUBLE(0)</f><v>0</v></c><c r="E1"><f>FACTDOUBLE(1)</f><v>0</v></c><c r="F1"><f>FACTDOUBLE(-1)</f><v>7</v></c><c r="G1"><f>FACTDOUBLE(301)</f><v>8</v></c><c r="H1"><f>FACTDOUBLE(&quot;ab&quot;)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "6").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>FACTDOUBLE(A1)</f><v>48</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FACTDOUBLE(7.9)</f><v>105</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>FACTDOUBLE(0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>FACTDOUBLE(1)</f><v>1</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>FACTDOUBLE(-1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FACTDOUBLE(301)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FACTDOUBLE(&quot;ab&quot;)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_poisson() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>5</v></c><c r="C1"><f>POISSON.DIST(A1,B1,0)</f><v>0</v></c><c r="D1"><f>POISSON(A1,B1,1)</f><v>0</v></c><c r="E1"><f>POISSON.DIST(2.9,5,0)</f><v>0</v></c><c r="F1"><f>POISSON.DIST(0,0,0)</f><v>0</v></c><c r="G1"><f>POISSON.DIST(A1,0,1)</f><v>0</v></c><c r="H1"><f>POISSON.DIST(-1,5,0)</f><v>7</v></c><c r="I1"><f>POISSON.DIST(2,-1,0)</f><v>8</v></c><c r="J1"><f>POISSON.DIST(171,1,0)</f><v>9</v></c><c r="K1"><f>POISSON.DIST(2,700,0)</f><v>11</v></c><c r="L1"><f>POISSON.DIST(2,5)</f><v>12</v></c><c r="M1"><f>POISSON.DIST(&quot;ab&quot;,5,0)</f><v>13</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>POISSON.DIST(A1,B1,0)</f><v>0.08422434</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>POISSON(A1,B1,1)</f><v>0.12465202</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>POISSON.DIST(2.9,5,0)</f><v>0.08422434</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>POISSON.DIST(0,0,0)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>POISSON.DIST(A1,0,1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>POISSON.DIST(-1,5,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>POISSON.DIST(2,-1,0)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>POISSON.DIST(171,1,0)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>POISSON.DIST(2,700,0)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>POISSON.DIST(2,5)</f><v>12</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>POISSON.DIST(&quot;ab&quot;,5,0)</f><v>13</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_binom_dist() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>4</v></c><c r="C1"><v>0.5</v></c><c r="D1"><f>BINOM.DIST(A1,B1,C1,0)</f><v>0</v></c><c r="E1"><f>BINOM.DIST(A1,B1,C1,1)</f><v>0</v></c><c r="F1"><f>BINOM.DIST(2.9,4.9,0.5,0)</f><v>0</v></c><c r="G1"><f>BINOMDIST(0,4,0,0)</f><v>0</v></c><c r="H1"><f>BINOM.DIST(1,4,0,0)</f><v>0</v></c><c r="I1"><f>BINOM.DIST(4,4,1,0)</f><v>0</v></c><c r="J1"><f>BINOM.DIST(3,4,1.1,0)</f><v>7</v></c><c r="K1"><f>BINOM.DIST(5,4,0.5,0)</f><v>8</v></c><c r="L1"><f>BINOM.DIST(2,171,0.5,0)</f><v>9</v></c><c r="M1"><f>BINOM.DIST(2,4,0.5)</f><v>11</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>BINOM.DIST(A1,B1,C1,0)</f><v>0.375</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST(A1,B1,C1,1)</f><v>0.6875</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST(2.9,4.9,0.5,0)</f><v>0.375</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOMDIST(0,4,0,0)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST(1,4,0,0)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST(4,4,1,0)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST(3,4,1.1,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST(5,4,0.5,0)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST(2,171,0.5,0)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST(2,4,0.5)</f><v>11</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_expon_dist() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>2</v></c><c r="C1"><f>EXPON.DIST(A1,B1,0)</f><v>0</v></c><c r="D1"><f>EXPON.DIST(A1,B1,1)</f><v>0</v></c><c r="E1"><f>EXPONDIST(0,2,0)</f><v>0</v></c><c r="F1"><f>EXPON.DIST(0,2,1)</f><v>0</v></c><c r="G1"><f>EXPON.DIST(-1,2,0)</f><v>7</v></c><c r="H1"><f>EXPON.DIST(1,0,0)</f><v>8</v></c><c r="I1"><f>EXPON.DIST(1,-2,1)</f><v>9</v></c><c r="J1"><f>EXPON.DIST(1,2)</f><v>11</v></c><c r="K1"><f>EXPON.DIST(&quot;ab&quot;,2,0)</f><v>12</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "0.5").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>EXPON.DIST(A1,B1,0)</f><v>0.73575888</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>EXPON.DIST(A1,B1,1)</f><v>0.63212056</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>EXPONDIST(0,2,0)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>EXPON.DIST(0,2,1)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>EXPON.DIST(-1,2,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>EXPON.DIST(1,0,0)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>EXPON.DIST(1,-2,1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>EXPON.DIST(1,2)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>EXPON.DIST(&quot;ab&quot;,2,0)</f><v>12</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_negbinom_dist() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>3</v></c><c r="C1"><f>NEGBINOM.DIST(A1,B1,0.5,0)</f><v>0</v></c><c r="D1"><f>NEGBINOM.DIST(A1,B1,0.5,1)</f><v>0</v></c><c r="E1"><f>NEGBINOMDIST(2,3,0.5)</f><v>0</v></c><c r="F1"><f>NEGBINOM.DIST(2.9,3.2,0.5,0)</f><v>0</v></c><c r="G1"><f>NEGBINOM.DIST(0,1,0.5,0)</f><v>0</v></c><c r="H1"><f>NEGBINOM.DIST(-1,3,0.5,0)</f><v>7</v></c><c r="I1"><f>NEGBINOM.DIST(2,0,0.5,0)</f><v>8</v></c><c r="J1"><f>NEGBINOM.DIST(2,3,0,0)</f><v>9</v></c><c r="K1"><f>NEGBINOM.DIST(2,3,1,1)</f><v>10</v></c><c r="L1"><f>NEGBINOM.DIST(171,1,0.5,0)</f><v>11</v></c><c r="M1"><f>NEGBINOM.DIST(2,3,0.5)</f><v>12</v></c><c r="N1"><f>NEGBINOM.DIST(&quot;ab&quot;,3,0.5,0)</f><v>13</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>NEGBINOM.DIST(A1,B1,0.5,0)</f><v>0.1875</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NEGBINOM.DIST(A1,B1,0.5,1)</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NEGBINOMDIST(2,3,0.5)</f><v>0.1875</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NEGBINOM.DIST(2.9,3.2,0.5,0)</f><v>0.1875</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NEGBINOM.DIST(0,1,0.5,0)</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NEGBINOM.DIST(-1,3,0.5,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NEGBINOM.DIST(2,0,0.5,0)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NEGBINOM.DIST(2,3,0,0)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NEGBINOM.DIST(2,3,1,1)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NEGBINOM.DIST(171,1,0.5,0)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NEGBINOM.DIST(2,3,0.5)</f><v>12</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NEGBINOM.DIST(&quot;ab&quot;,3,0.5,0)</f><v>13</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_hypgeom_dist() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>5</v></c><c r="C1"><f>HYPGEOM.DIST(A1,B1,4,10,0)</f><v>0</v></c><c r="D1"><f>HYPGEOM.DIST(A1,B1,4,10,1)</f><v>0</v></c><c r="E1"><f>HYPGEOMDIST(2,5,4,10)</f><v>0</v></c><c r="F1"><f>HYPGEOM.DIST(2.9,5.2,4.9,10.8,0)</f><v>0</v></c><c r="G1"><f>HYPGEOM.DIST(0,0,0,0,0)</f><v>0</v></c><c r="H1"><f>HYPGEOM.DIST(-1,5,4,10,0)</f><v>7</v></c><c r="I1"><f>HYPGEOM.DIST(6,5,4,10,0)</f><v>8</v></c><c r="J1"><f>HYPGEOM.DIST(1,11,4,10,0)</f><v>9</v></c><c r="K1"><f>HYPGEOM.DIST(1,5,8,10,0)</f><v>10</v></c><c r="L1"><f>HYPGEOM.DIST(0,171,0,171,0)</f><v>11</v></c><c r="M1"><f>HYPGEOM.DIST(2,5,4,10)</f><v>12</v></c><c r="N1"><f>HYPGEOM.DIST(&quot;ab&quot;,5,4,10,0)</f><v>13</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>HYPGEOM.DIST(A1,B1,4,10,0)</f><v>0.47619048</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HYPGEOM.DIST(A1,B1,4,10,1)</f><v>0.73809524</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HYPGEOMDIST(2,5,4,10)</f><v>0.47619048</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HYPGEOM.DIST(2.9,5.2,4.9,10.8,0)</f><v>0.47619048</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HYPGEOM.DIST(0,0,0,0,0)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HYPGEOM.DIST(-1,5,4,10,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HYPGEOM.DIST(6,5,4,10,0)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HYPGEOM.DIST(1,11,4,10,0)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HYPGEOM.DIST(1,5,8,10,0)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HYPGEOM.DIST(0,171,0,171,0)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HYPGEOM.DIST(2,5,4,10)</f><v>12</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HYPGEOM.DIST(&quot;ab&quot;,5,4,10,0)</f><v>13</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_weibull_dist() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>2</v></c><c r="C1"><v>2</v></c><c r="D1"><f>WEIBULL.DIST(A1,B1,C1,0)</f><v>0</v></c><c r="E1"><f>WEIBULL.DIST(A1,B1,C1,1)</f><v>0</v></c><c r="F1"><f>WEIBULL(0.5,1,0.5,0)</f><v>0</v></c><c r="G1"><f>WEIBULL.DIST(0,2,1,0)</f><v>0</v></c><c r="H1"><f>WEIBULL.DIST(0,2,1,1)</f><v>0</v></c><c r="I1"><f>WEIBULL.DIST(0,1,2,0)</f><v>0</v></c><c r="J1"><f>WEIBULL.DIST(0,0.5,1,0)</f><v>7</v></c><c r="K1"><f>WEIBULL.DIST(0,0.5,1,1)</f><v>0</v></c><c r="L1"><f>WEIBULL.DIST(-1,2,1,0)</f><v>8</v></c><c r="M1"><f>WEIBULL.DIST(1,0,1,0)</f><v>9</v></c><c r="N1"><f>WEIBULL.DIST(1,2,0,1)</f><v>10</v></c><c r="O1"><f>WEIBULL.DIST(10000000000000000,20,1,0)</f><v>11</v></c><c r="P1"><f>WEIBULL.DIST(1,2,1)</f><v>12</v></c><c r="Q1"><f>WEIBULL.DIST(&quot;ab&quot;,2,1,0)</f><v>13</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>WEIBULL.DIST(A1,B1,C1,0)</f><v>0.36787944</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WEIBULL.DIST(A1,B1,C1,1)</f><v>0.63212056</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WEIBULL(0.5,1,0.5,0)</f><v>0.73575888</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WEIBULL.DIST(0,2,1,0)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WEIBULL.DIST(0,2,1,1)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WEIBULL.DIST(0,1,2,0)</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WEIBULL.DIST(0,0.5,1,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WEIBULL.DIST(0,0.5,1,1)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WEIBULL.DIST(-1,2,1,0)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WEIBULL.DIST(1,0,1,0)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WEIBULL.DIST(1,2,0,1)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WEIBULL.DIST(10000000000000000,20,1,0)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WEIBULL.DIST(1,2,1)</f><v>12</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WEIBULL.DIST(&quot;ab&quot;,2,1,0)</f><v>13</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_gamma() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>GAMMA(A1)</f><v>0</v></c><c r="C1"><f>GAMMALN(A1)</f><v>0</v></c><c r="D1"><f>GAMMA(0.5)</f><v>0</v></c><c r="E1"><f>GAMMA(-0.5)</f><v>0</v></c><c r="F1"><f>GAMMALN(0.5)</f><v>0</v></c><c r="G1"><f>GAMMA(1)</f><v>0</v></c><c r="H1"><f>GAMMALN(1)</f><v>0</v></c><c r="I1"><f>GAMMA(0)</f><v>7</v></c><c r="J1"><f>GAMMA(-2)</f><v>8</v></c><c r="K1"><f>GAMMA(171)</f><v>9</v></c><c r="L1"><f>GAMMALN(0)</f><v>10</v></c><c r="M1"><f>GAMMALN(-1)</f><v>11</v></c><c r="N1"><f>GAMMA(&quot;ab&quot;)</f><v>12</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "5").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>GAMMA(A1)</f><v>24</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>GAMMALN(A1)</f><v>3.17805383</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA(0.5)</f><v>1.77245385</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA(-0.5)</f><v>-3.5449077</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMALN(0.5)</f><v>0.57236494</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>GAMMA(1)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GAMMALN(1)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GAMMA(0)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GAMMA(-2)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GAMMA(171)</f><v>9</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GAMMALN(0)</f><v>10</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GAMMALN(-1)</f><v>11</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>GAMMA(&quot;ab&quot;)</f><v>12</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_gamma_dist() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>2</v></c><c r="C1"><v>1</v></c><c r="D1"><f>GAMMA.DIST(A1,B1,C1,0)</f><v>0</v></c><c r="E1"><f>GAMMA.DIST(A1,B1,C1,1)</f><v>0</v></c><c r="F1"><f>GAMMADIST(0.5,1,0.5,0)</f><v>0</v></c><c r="G1"><f>GAMMA.DIST(1,0.5,1,0)</f><v>0</v></c><c r="H1"><f>GAMMA.DIST(1,1.5,1,1)</f><v>0</v></c><c r="I1"><f>GAMMA.DIST(0,2,1,0)</f><v>0</v></c><c r="J1"><f>GAMMA.DIST(0,2,1,1)</f><v>0</v></c><c r="K1"><f>GAMMA.DIST(0,1,2,0)</f><v>0</v></c><c r="L1"><f>GAMMA.DIST(0,0.5,1,0)</f><v>7</v></c><c r="M1"><f>GAMMA.DIST(-1,2,1,0)</f><v>8</v></c><c r="N1"><f>GAMMA.DIST(1,0,1,0)</f><v>9</v></c><c r="O1"><f>GAMMA.DIST(1,2,0,1)</f><v>10</v></c><c r="P1"><f>GAMMA.DIST(1,172,1,1)</f><v>11</v></c><c r="Q1"><f>GAMMA.DIST(1000,0.5,1,1)</f><v>12</v></c><c r="R1"><f>GAMMA.DIST(1,2,1)</f><v>13</v></c><c r="S1"><f>GAMMA.DIST(&quot;ab&quot;,2,1,0)</f><v>14</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(A1,B1,C1,0)</f><v>0.36787944</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(A1,B1,C1,1)</f><v>0.26424112</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMADIST(0.5,1,0.5,0)</f><v>0.73575888</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(1,0.5,1,0)</f><v>0.20755375</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(1,1.5,1,1)</f><v>0.4275933</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(0,2,1,0)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(0,2,1,1)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(0,1,2,0)</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(0,0.5,1,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(-1,2,1,0)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(1,0,1,0)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(1,2,0,1)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(1,172,1,1)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(1000,0.5,1,1)</f><v>12</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(1,2,1)</f><v>13</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(&quot;ab&quot;,2,1,0)</f><v>14</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_binom_inv() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>0.5</v></c><c r="C1"><f>BINOM.INV(A1,B1,0.5)</f><v>0</v></c><c r="D1"><f>BINOM.INV(A1,B1,0.6875)</f><v>0</v></c><c r="E1"><f>BINOM.INV(A1,B1,0.3125)</f><v>0</v></c><c r="F1"><f>CRITBINOM(4,0.5,0.0625)</f><v>0</v></c><c r="G1"><f>BINOM.INV(4.9,0.5,0.5)</f><v>0</v></c><c r="H1"><f>BINOM.INV(-1,0.5,0.5)</f><v>7</v></c><c r="I1"><f>BINOM.INV(4,0,0.5)</f><v>8</v></c><c r="J1"><f>BINOM.INV(4,1,0.5)</f><v>9</v></c><c r="K1"><f>BINOM.INV(4,0.5,0)</f><v>10</v></c><c r="L1"><f>BINOM.INV(4,0.5,1)</f><v>11</v></c><c r="M1"><f>BINOM.INV(171,0.5,0.5)</f><v>12</v></c><c r="N1"><f>BINOM.INV(4,0.5)</f><v>13</v></c><c r="O1"><f>BINOM.INV(&quot;ab&quot;,0.5,0.5)</f><v>14</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "4").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>BINOM.INV(A1,B1,0.5)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.INV(A1,B1,0.6875)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.INV(A1,B1,0.3125)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CRITBINOM(4,0.5,0.0625)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.INV(4.9,0.5,0.5)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.INV(-1,0.5,0.5)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.INV(4,0,0.5)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.INV(4,1,0.5)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.INV(4,0.5,0)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.INV(4,0.5,1)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.INV(171,0.5,0.5)</f><v>12</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.INV(4,0.5)</f><v>13</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.INV(&quot;ab&quot;,0.5,0.5)</f><v>14</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_chisq_dist() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>2</v></c><c r="C1"><f>CHISQ.DIST(A1,B1,0)</f><v>0</v></c><c r="D1"><f>CHISQ.DIST(A1,B1,1)</f><v>0</v></c><c r="E1"><f>CHISQ.DIST.RT(A1,B1)</f><v>0</v></c><c r="F1"><f>CHIDIST(1,2)</f><v>0</v></c><c r="G1"><f>CHISQ.DIST(1,2.9,0)</f><v>0</v></c><c r="H1"><f>CHISQ.DIST(1,1,1)</f><v>0</v></c><c r="I1"><f>CHISQ.DIST(-1,2,0)</f><v>7</v></c><c r="J1"><f>CHISQ.DIST(1,0.9,0)</f><v>8</v></c><c r="K1"><f>CHISQ.DIST(1,344,1)</f><v>9</v></c><c r="L1"><f>CHISQ.DIST(1400,2,1)</f><v>10</v></c><c r="M1"><f>CHISQ.DIST(2000,1,1)</f><v>11</v></c><c r="N1"><f>CHISQ.DIST(1,2)</f><v>12</v></c><c r="O1"><f>CHISQ.DIST(&quot;ab&quot;,2,0)</f><v>13</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>CHISQ.DIST(A1,B1,0)</f><v>0.30326533</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.DIST(A1,B1,1)</f><v>0.39346934</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.DIST.RT(A1,B1)</f><v>0.60653066</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHIDIST(1,2)</f><v>0.60653066</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.DIST(1,2.9,0)</f><v>0.30326533</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.DIST(1,1,1)</f><v>0.68268949</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.DIST(-1,2,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.DIST(1,0.9,0)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.DIST(1,344,1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.DIST(1400,2,1)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.DIST(2000,1,1)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.DIST(1,2)</f><v>12</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.DIST(&quot;ab&quot;,2,0)</f><v>13</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_norms_dist() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>NORM.S.DIST(A1,0)</f><v>0</v></c><c r="C1"><f>NORM.S.DIST(A1,1)</f><v>0</v></c><c r="D1"><f>NORMSDIST(1)</f><v>0</v></c><c r="E1"><f>NORM.S.DIST(0,0)</f><v>0</v></c><c r="F1"><f>NORM.S.DIST(0,1)</f><v>0</v></c><c r="G1"><f>NORM.S.DIST(-1,1)</f><v>0</v></c><c r="H1"><f>NORM.S.DIST(40,0)</f><v>7</v></c><c r="I1"><f>NORM.S.DIST(40,1)</f><v>8</v></c><c r="J1"><f>NORM.S.DIST(1)</f><v>9</v></c><c r="K1"><f>NORM.S.DIST(&quot;ab&quot;,1)</f><v>10</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>NORM.S.DIST(A1,0)</f><v>0.24197072</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.S.DIST(A1,1)</f><v>0.84134475</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORMSDIST(1)</f><v>0.84134475</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.S.DIST(0,0)</f><v>0.39894228</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.S.DIST(0,1)</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.S.DIST(-1,1)</f><v>0.15865525</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.S.DIST(40,0)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.S.DIST(40,1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.S.DIST(1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.S.DIST(&quot;ab&quot;,1)</f><v>10</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_norm_dist() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>40</v></c><c r="C1"><v>1.5</v></c><c r="D1"><f>NORM.DIST(A1,B1,C1,0)</f><v>0</v></c><c r="E1"><f>NORM.DIST(A1,B1,C1,1)</f><v>0</v></c><c r="F1"><f>NORMDIST(1,0,1,0)</f><v>0</v></c><c r="G1"><f>NORM.DIST(1,0,1,1)</f><v>0</v></c><c r="H1"><f>NORM.DIST(40,0,1,0)</f><v>7</v></c><c r="I1"><f>NORM.DIST(40,0,1,1)</f><v>8</v></c><c r="J1"><f>NORM.DIST(1,0,0,1)</f><v>9</v></c><c r="K1"><f>NORM.DIST(1,0,-1,1)</f><v>10</v></c><c r="L1"><f>NORM.DIST(1,0,1)</f><v>11</v></c><c r="M1"><f>NORM.DIST(&quot;ab&quot;,0,1,0)</f><v>12</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "42").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>NORM.DIST(A1,B1,C1,0)</f><v>0.10934005</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.DIST(A1,B1,C1,1)</f><v>0.90878878</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORMDIST(1,0,1,0)</f><v>0.24197072</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.DIST(1,0,1,1)</f><v>0.84134475</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.DIST(40,0,1,0)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.DIST(40,0,1,1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.DIST(1,0,0,1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.DIST(1,0,-1,1)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.DIST(1,0,1)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.DIST(&quot;ab&quot;,0,1,0)</f><v>12</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_erf() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>ERF(A1)</f><v>0</v></c><c r="C1"><f>ERF(0,A1)</f><v>0</v></c><c r="D1"><f>ERF(-1,1)</f><v>0</v></c><c r="E1"><f>ERFC(A1)</f><v>0</v></c><c r="F1"><f>GAUSS(A1)</f><v>0</v></c><c r="G1"><f>PHI(0)</f><v>0</v></c><c r="H1"><f>PHI(A1)</f><v>0</v></c><c r="I1"><f>ERF(40)</f><v>7</v></c><c r="J1"><f>ERF(&quot;ab&quot;)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>ERF(A1)</f><v>0.84270079</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ERF(0,A1)</f><v>0.84270079</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ERF(-1,1)</f><v>1.68540159</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ERFC(A1)</f><v>0.15729921</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAUSS(A1)</f><v>0.34134475</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PHI(0)</f><v>0.39894228</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PHI(A1)</f><v>0.24197072</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>ERF(40)</f><v>7</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>ERF(&quot;ab&quot;)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_lognorm_dist() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>LOGNORM.DIST(A1,0,1,0)</f><v>0</v></c><c r="C1"><f>LOGNORM.DIST(A1,0,1,1)</f><v>0</v></c><c r="D1"><f>LOGNORMDIST(1,0,1)</f><v>0</v></c><c r="E1"><f>LOGNORM.DIST(1,0,1,0)</f><v>0</v></c><c r="F1"><f>LOGNORM.DIST(0,0,1,1)</f><v>7</v></c><c r="G1"><f>LOGNORM.DIST(-1,0,1,0)</f><v>8</v></c><c r="H1"><f>LOGNORM.DIST(2,0,0,1)</f><v>9</v></c><c r="I1"><f>LOGNORM.DIST(2,0,-1,1)</f><v>10</v></c><c r="J1"><f>LOGNORM.DIST(2,0,1)</f><v>11</v></c><c r="K1"><f>LOGNORM.DIST(&quot;ab&quot;,0,1,0)</f><v>12</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>LOGNORM.DIST(A1,0,1,0)</f><v>0.15687402</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGNORM.DIST(A1,0,1,1)</f><v>0.7558914</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGNORMDIST(1,0,1)</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGNORM.DIST(1,0,1,0)</f><v>0.39894228</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGNORM.DIST(0,0,1,1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGNORM.DIST(-1,0,1,0)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGNORM.DIST(2,0,0,1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGNORM.DIST(2,0,-1,1)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGNORM.DIST(2,0,1)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGNORM.DIST(&quot;ab&quot;,0,1,0)</f><v>12</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_binom_range() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>BINOM.DIST.RANGE(4,0.5,A1,2)</f><v>0</v></c><c r="C1"><f>BINOM.DIST.RANGE(4,0.5,2)</f><v>0</v></c><c r="D1"><f>BINOM.DIST.RANGE(4.9,0.5,1.9,2.2)</f><v>0</v></c><c r="E1"><f>BINOM.DIST.RANGE(4,0.5,3,1)</f><v>7</v></c><c r="F1"><f>BINOM.DIST.RANGE(4,0.5,5,5)</f><v>8</v></c><c r="G1"><f>BINOM.DIST.RANGE(4,1.5,1,2)</f><v>9</v></c><c r="H1"><f>BINOM.DIST.RANGE(171,0.5,0,1)</f><v>10</v></c><c r="I1"><f>BINOM.DIST.RANGE(4,0.5)</f><v>11</v></c><c r="J1"><f>BINOM.DIST.RANGE(&quot;ab&quot;,0.5,1,2)</f><v>12</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>BINOM.DIST.RANGE(4,0.5,A1,2)</f><v>0.625</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST.RANGE(4,0.5,2)</f><v>0.375</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST.RANGE(4.9,0.5,1.9,2.2)</f><v>0.625</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST.RANGE(4,0.5,3,1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST.RANGE(4,0.5,5,5)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST.RANGE(4,1.5,1,2)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST.RANGE(171,0.5,0,1)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST.RANGE(4,0.5)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST.RANGE(&quot;ab&quot;,0.5,1,2)</f><v>12</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_z_test() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>1</v></c><c r="C1"><v>1</v></c><c r="D1"><v>1</v></c><c r="E1"><f>Z.TEST(A1:D1,0,2)</f><v>0</v></c><c r="F1"><f>Z.TEST(A1:D1,1)</f><v>7</v></c><c r="G1"><f>ZTEST(A2:C2,0)</f><v>0</v></c><c r="H1"><f>Z.TEST(A1,0,1)</f><v>8</v></c><c r="I1"><f>Z.TEST(A1:D1,0,0)</f><v>9</v></c><c r="J1"><f>Z.TEST(A1:D1,&quot;ab&quot;,1)</f><v>10</v></c></row><row r="2"><c r="A2"><v>1</v></c><c r="B2"><v>2</v></c><c r="C2"><v>3</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>Z.TEST(A1:D1,0,2)</f><v>0.15865525</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>Z.TEST(A1:D1,1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ZTEST(A2:C2,0)</f><v>0.000266</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>Z.TEST(A1,0,1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>Z.TEST(A1:D1,0,0)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>Z.TEST(A1:D1,&quot;ab&quot;,1)</f><v>10</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_prob() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>2</v></c><c r="C1"><v>3</v></c><c r="D1"><f>PROB(A1:C1,A2:C2,2)</f><v>0</v></c><c r="E1"><f>PROB(A1:C1,A2:C2,1,2)</f><v>0</v></c><c r="F1"><f>PROB(A1:C1,A2:C2,4)</f><v>0</v></c><c r="G1"><f>PROB(A1:C1,A2:C2,2,1)</f><v>7</v></c><c r="H1"><f>PROB(A1:C1,D2:F2,1)</f><v>8</v></c><c r="I1"><f>PROB(A1,A2,1)</f><v>9</v></c><c r="J1"><f>PROB(A1:C1,A2:C2,&quot;ab&quot;)</f><v>10</v></c></row><row r="2"><c r="A2"><v>0.2</v></c><c r="B2"><v>0</v></c><c r="C2"><v>0.3</v></c><c r="D2"><v>-0.1</v></c><c r="E2"><v>0.2</v></c><c r="F2"><v>0.3</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "B2", "0.5").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>PROB(A1:C1,A2:C2,2)</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PROB(A1:C1,A2:C2,1,2)</f><v>0.7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PROB(A1:C1,A2:C2,4)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PROB(A1:C1,A2:C2,2,1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PROB(A1:C1,D2:F2,1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>PROB(A1,A2,1)</f><v>9</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>PROB(A1:C1,A2:C2,&quot;ab&quot;)</f><v>10</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_inverse_hyperbolic() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>0.5</v></c><c r="C1"><v>-1</v></c><c r="D1"><f>ASINH(A1)</f><v>9</v></c><c r="E1"><f>ACOSH(A1)</f><v>8</v></c><c r="F1"><f>ATANH(B1)</f><v>7</v></c><c r="G1"><f>ASINH(C1)</f><v>6</v></c><c r="H1"><f>ACOSH(0.5)</f><v>5</v></c><c r="I1"><f>ATANH(1)</f><v>4</v></c><c r="J1"><f>ATANH(-1)</f><v>3</v></c><c r="K1"><f>ATANH(2)</f><v>2</v></c><c r="L1"><f>ASINH(&quot;ab&quot;)</f><v>1</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>ASINH(A1)</f><v>0.88137359</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>ACOSH(A1)</f><v>0</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>ATANH(B1)</f><v>0.54930614</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ASINH(C1)</f><v>-0.88137359</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>ACOSH(0.5)</f><v>5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ATANH(1)</f><v>4</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ATANH(-1)</f><v>3</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ATANH(2)</f><v>2</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>ASINH(&quot;ab&quot;)</f><v>1</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_reciprocal_trig() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>SEC(A1)</f><v>9</v></c><c r="C1"><f>CSC(A1)</f><v>8</v></c><c r="D1"><f>COT(A1)</f><v>7</v></c><c r="E1"><f>SEC(0)</f><v>6</v></c><c r="F1"><f>CSC(0)</f><v>5</v></c><c r="G1"><f>COT(0)</f><v>4</v></c><c r="H1"><f>SEC(&quot;ab&quot;)</f><v>3</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SEC(A1)</f><v>1.85081572</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CSC(A1)</f><v>1.18839511</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COT(A1)</f><v>0.64209262</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>SEC(0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>CSC(0)</f><v>5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COT(0)</f><v>4</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>SEC(&quot;ab&quot;)</f><v>3</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_unichar() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>64</v></c><c r="B1"><f>CONCATENATE("a","b")</f><v>0</v></c><c r="C1"><f>UNICHAR(A1)</f><v>0</v></c><c r="D1"><f>UNICODE("AB")</f><v>0</v></c><c r="E1"><f>CONCATENATE(A1,"x")</f><v>0</v></c><c r="F1"><f>UNICHAR(0)</f><v>4</v></c><c r="G1"><f>UNICHAR(55296)</f><v>5</v></c><c r="H1"><f>UNICODE("")</f><v>6</v></c><c r="I1"><f>UNICHAR("ab")</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "65").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(
                r#"<c r="B1" t="inlineStr"><f>CONCATENATE("a","b")</f><is><t>ab</t></is></c>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="C1" t="inlineStr"><f>UNICHAR(A1)</f><is><t>A</t></is></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>UNICODE("AB")</f><v>65</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<c r="E1" t="inlineStr"><f>CONCATENATE(A1,"x")</f><is><t>65x</t></is></c>"#
            ),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>UNICHAR(0)</f><v>4</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>UNICHAR(55296)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>UNICODE("")</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>UNICHAR("ab")</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_series_sum() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>0</v></c><c r="C1"><v>3</v></c><c r="D1"><f>SERIESSUM(2,1,1,A1:C1)</f><v>0</v></c><c r="E1"><f>SERIESSUM(2,1,1,A2:C2)</f><v>0</v></c><c r="F1"><f>SERIESSUM(-2,0.5,1,A1:A1)</f><v>7</v></c><c r="G1"><f>SERIESSUM(0,0,1,A1:A1)</f><v>0</v></c><c r="H1"><f>SERIESSUM(2,-1,1,A1:A1)</f><v>0</v></c><c r="I1"><f>SERIESSUM(2,1,1,A1)</f><v>8</v></c><c r="J1"><f>SERIESSUM("ab",1,1,A1:C1)</f><v>9</v></c><c r="K1"><f>SERIESSUM(2,1,1,D2:F2)</f><v>10</v></c></row><row r="2"><c r="A2"><v>1</v></c><c r="C2"><v>3</v></c><c r="D2"><v>1</v></c><c r="E2" t="inlineStr"><is><t>xy</t></is></c><c r="F2"><v>3</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "B1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SERIESSUM(2,1,1,A1:C1)</f><v>34</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SERIESSUM(2,1,1,A2:C2)</f><v>26</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SERIESSUM(-2,0.5,1,A1:A1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SERIESSUM(0,0,1,A1:A1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SERIESSUM(2,-1,1,A1:A1)</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SERIESSUM(2,1,1,A1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SERIESSUM("ab",1,1,A1:C1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SERIESSUM(2,1,1,D2:F2)</f><v>10</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_norm_inv() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0.5</v></c><c r="B1"><f>NORMSINV(A1)</f><v>0</v></c><c r="C1"><f>NORM.S.INV(0.5)</f><v>9</v></c><c r="D1"><f>NORM.INV(A1,10,2)</f><v>0</v></c><c r="E1"><f>NORMINV(0.5,10,2)</f><v>0</v></c><c r="F1"><f>NORMSINV(0.025)</f><v>0</v></c><c r="G1"><f>NORMSINV(0)</f><v>4</v></c><c r="H1"><f>NORMSINV(1)</f><v>5</v></c><c r="I1"><f>NORM.INV(0.5,10,0)</f><v>6</v></c><c r="J1"><f>NORM.INV(0.5,10,-1)</f><v>7</v></c><c r="K1"><f>NORMSINV(&quot;ab&quot;)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "0.975").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>NORMSINV(A1)</f><v>1.95996398</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.S.INV(0.5)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.INV(A1,10,2)</f><v>13.91992797</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORMINV(0.5,10,2)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORMSINV(0.025)</f><v>-1.95996398</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>NORMSINV(0)</f><v>4</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>NORMSINV(1)</f><v>5</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>NORM.INV(0.5,10,0)</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.INV(0.5,10,-1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORMSINV(&quot;ab&quot;)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_lognorm_inv() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>LOGNORM.INV(0.5,A1,1)</f><v>0</v></c><c r="C1"><f>LOGINV(0.5,0,1)</f><v>0</v></c><c r="D1"><f>LOGNORM.INV(0.975,0,1)</f><v>0</v></c><c r="E1"><f>CONFIDENCE(0.05,1,1)</f><v>0</v></c><c r="F1"><f>CONFIDENCE.NORM(0.05,2,4)</f><v>0</v></c><c r="G1"><f>CONFIDENCE(0.05,1,1.9)</f><v>0</v></c><c r="H1"><f>CONFIDENCE(0,1,1)</f><v>4</v></c><c r="I1"><f>CONFIDENCE(1,1,1)</f><v>5</v></c><c r="J1"><f>CONFIDENCE(0.05,0,1)</f><v>6</v></c><c r="K1"><f>CONFIDENCE(0.05,1,0.9)</f><v>7</v></c><c r="L1"><f>LOGNORM.INV(0,0,1)</f><v>8</v></c><c r="M1"><f>LOGNORM.INV(0.5,0,0)</f><v>9</v></c><c r="N1"><f>CONFIDENCE(&quot;ab&quot;,1,1)</f><v>10</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>LOGNORM.INV(0.5,A1,1)</f><v>2.71828183</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGINV(0.5,0,1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGNORM.INV(0.975,0,1)</f><v>7.09907138</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONFIDENCE(0.05,1,1)</f><v>1.95996398</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONFIDENCE.NORM(0.05,2,4)</f><v>1.95996398</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONFIDENCE(0.05,1,1.9)</f><v>1.95996398</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONFIDENCE(0,1,1)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONFIDENCE(1,1,1)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONFIDENCE(0.05,0,1)</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONFIDENCE(0.05,1,0.9)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGNORM.INV(0,0,1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGNORM.INV(0.5,0,0)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONFIDENCE(&quot;ab&quot;,1,1)</f><v>10</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_gamma_inv() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>GAMMA.INV(A1,1,1)</f><v>0</v></c><c r="C1"><f>GAMMAINV(0.5,1,2)</f><v>0</v></c><c r="D1"><f>CHISQ.INV(A1,2)</f><v>0</v></c><c r="E1"><f>CHISQ.INV(0.5,2.9)</f><v>0</v></c><c r="F1"><f>CHISQ.INV.RT(0.5,2)</f><v>0</v></c><c r="G1"><f>CHIINV(0.05,2)</f><v>0</v></c><c r="H1"><f>GAMMA.INV(0,1,1)</f><v>4</v></c><c r="I1"><f>GAMMA.INV(1,1,1)</f><v>5</v></c><c r="J1"><f>GAMMA.INV(0.5,0,1)</f><v>6</v></c><c r="K1"><f>CHISQ.INV(0.5,0.9)</f><v>7</v></c><c r="L1"><f>CHISQ.INV(&quot;ab&quot;,2)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "0.5").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>GAMMA.INV(A1,1,1)</f><v>0.69314718</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMAINV(0.5,1,2)</f><v>1.38629436</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.INV(A1,2)</f><v>1.38629436</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.INV(0.5,2.9)</f><v>1.38629436</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.INV.RT(0.5,2)</f><v>1.38629436</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHIINV(0.05,2)</f><v>5.99146455</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.INV(0,1,1)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.INV(1,1,1)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.INV(0.5,0,1)</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.INV(0.5,0.9)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.INV(&quot;ab&quot;,2)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_roman() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>3</v></c><c r="B1"><f>ROMAN(A1)</f><v>0</v></c><c r="C1"><f>ROMAN(9)</f><v>0</v></c><c r="D1"><f>ROMAN(1990)</f><v>0</v></c><c r="E1"><f>ROMAN(3.9)</f><v>0</v></c><c r="F1"><f>ROMAN(0)</f><v>5</v></c><c r="G1"><f>ROMAN(4000)</f><v>6</v></c><c r="H1"><f>ROMAN(4,1)</f><v>7</v></c><c r="I1"><f>ROMAN(4,0)</f><v>0</v></c><c r="J1"><f>ARABIC("IV")</f><v>0</v></c><c r="K1"><f>ARABIC("ii")</f><v>0</v></c><c r="L1"><f>ARABIC("MCMXC")</f><v>0</v></c><c r="M1"><f>ARABIC("IIII")</f><v>8</v></c><c r="N1"><f>ARABIC("")</f><v>9</v></c><c r="O1"><f>ROMAN("ab")</f><v>10</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "4").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<c r="B1" t="inlineStr"><f>ROMAN(A1)</f><is><t>IV</t></is></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="C1" t="inlineStr"><f>ROMAN(9)</f><is><t>IX</t></is></c>"#),
            "{sheet}"
        );
        assert!(
            sheet
                .contains(r#"<c r="D1" t="inlineStr"><f>ROMAN(1990)</f><is><t>MCMXC</t></is></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="E1" t="inlineStr"><f>ROMAN(3.9)</f><is><t>III</t></is></c>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>ROMAN(0)</f><v>5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ROMAN(4000)</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ROMAN(4,1)</f><v>7</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<c r="I1" t="inlineStr"><f>ROMAN(4,0)</f><is><t>IV</t></is></c>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>ARABIC("IV")</f><v>4</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ARABIC("ii")</f><v>2</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>ARABIC("MCMXC")</f><v>1990</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ARABIC("IIII")</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>ARABIC("")</f><v>9</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ROMAN("ab")</f><v>10</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_reciprocal_hyper() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>SECH(A1)</f><v>0</v></c><c r="C1"><f>CSCH(A1)</f><v>0</v></c><c r="D1"><f>COTH(A1)</f><v>0</v></c><c r="E1"><f>SECH(0)</f><v>0</v></c><c r="F1"><f>CSCH(0)</f><v>7</v></c><c r="G1"><f>COTH(0)</f><v>8</v></c><c r="H1"><f>SECH(&quot;ab&quot;)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SECH(A1)</f><v>0.64805427</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CSCH(A1)</f><v>0.85091813</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COTH(A1)</f><v>1.31303529</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>SECH(0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>CSCH(0)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COTH(0)</f><v>8</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>SECH(&quot;ab&quot;)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_acot() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>ACOT(A1)</f><v>0</v></c><c r="C1"><f>ACOT(0)</f><v>0</v></c><c r="D1"><f>ACOT(-1)</f><v>0</v></c><c r="E1"><f>ACOTH(2)</f><v>0</v></c><c r="F1"><f>ACOTH(-2)</f><v>0</v></c><c r="G1"><f>ACOTH(1)</f><v>7</v></c><c r="H1"><f>ACOTH(0.5)</f><v>8</v></c><c r="I1"><f>ACOT(&quot;ab&quot;)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>ACOT(A1)</f><v>0.78539816</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ACOT(0)</f><v>1.57079633</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ACOT(-1)</f><v>2.35619449</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ACOTH(2)</f><v>0.54930614</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ACOTH(-2)</f><v>-0.54930614</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>ACOTH(1)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ACOTH(0.5)</f><v>8</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>ACOT(&quot;ab&quot;)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_chisq_test() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>2</v></c><c r="C1"><v>2</v></c><c r="D1"><v>1</v></c><c r="E1"><f>CHISQ.TEST(A1:B1,C1:D1)</f><v>0</v></c><c r="F1"><f>CHITEST(A1:B1,A1:B1)</f><v>0</v></c><c r="G1"><f>CHISQ.TEST(A2:B3,C2:D3)</f><v>0</v></c><c r="H1"><f>CHISQ.TEST(A1:A1,C1:C1)</f><v>7</v></c><c r="I1"><f>CHISQ.TEST(A1:B1,A2:A3)</f><v>8</v></c><c r="J1"><f>CHISQ.TEST(A1:B1,E2:F2)</f><v>9</v></c><c r="K1"><f>CHISQ.TEST(A1:B1,G2:H2)</f><v>10</v></c><c r="L1"><f>CHISQ.TEST(1,2)</f><v>11</v></c></row><row r="2"><c r="A2"><v>1</v></c><c r="B2"><v>2</v></c><c r="C2"><v>1</v></c><c r="D2"><v>2</v></c><c r="E2"><v>0</v></c><c r="F2"><v>1</v></c><c r="G2" t="inlineStr"><is><t>xy</t></is></c><c r="H2"><v>1</v></c></row><row r="3"><c r="A3"><v>3</v></c><c r="B3"><v>4</v></c><c r="C3"><v>3</v></c><c r="D3"><v>4</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>CHISQ.TEST(A1:B1,C1:D1)</f><v>0.22067136</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHITEST(A1:B1,A1:B1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.TEST(A2:B3,C2:D3)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.TEST(A1:A1,C1:C1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.TEST(A1:B1,A2:A3)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.TEST(A1:B1,E2:F2)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.TEST(A1:B1,G2:H2)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.TEST(1,2)</f><v>11</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_percentile_exc() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>20</v></c><c r="C1"><v>30</v></c><c r="D1"><v>40</v></c><c r="E1" t="inlineStr"><is><t>xy</t></is></c><c r="F1"><f>PERCENTILE.EXC(A1:E1,0.25)</f><v>0</v></c><c r="G1"><f>PERCENTILE.EXC(A1:E1,0.5)</f><v>0</v></c><c r="H1"><f>PERCENTILE.EXC(A1:E1,0.75)</f><v>0</v></c><c r="I1"><f>PERCENTILE.EXC(A1:E1,0.2)</f><v>0</v></c><c r="J1"><f>PERCENTILE.EXC(A1:E1,0.8)</f><v>0</v></c><c r="K1"><f>PERCENTILE.EXC(A1:E1,0)</f><v>7</v></c><c r="L1"><f>PERCENTILE.EXC(A1:E1,1)</f><v>8</v></c><c r="M1"><f>PERCENTILE.EXC(A1:E1,0.1)</f><v>9</v></c><c r="N1"><f>PERCENTILE.EXC(A1,0.5)</f><v>11</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "10").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>PERCENTILE.EXC(A1:E1,0.25)</f><v>12.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE.EXC(A1:E1,0.5)</f><v>25</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE.EXC(A1:E1,0.75)</f><v>37.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE.EXC(A1:E1,0.2)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE.EXC(A1:E1,0.8)</f><v>40</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE.EXC(A1:E1,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE.EXC(A1:E1,1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE.EXC(A1:E1,0.1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE.EXC(A1,0.5)</f><v>11</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_quartile_exc() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>20</v></c><c r="C1"><v>30</v></c><c r="D1"><v>40</v></c><c r="E1" t="inlineStr"><is><t>xy</t></is></c><c r="F1"><f>QUARTILE.EXC(A1:E1,1)</f><v>0</v></c><c r="G1"><f>QUARTILE.EXC(A1:E1,1.9)</f><v>0</v></c><c r="H1"><f>QUARTILE.EXC(A1:E1,2)</f><v>0</v></c><c r="I1"><f>QUARTILE.EXC(A1:E1,3)</f><v>0</v></c><c r="J1"><f>QUARTILE.EXC(A1:E1,0)</f><v>7</v></c><c r="K1"><f>QUARTILE.EXC(A1:E1,4)</f><v>8</v></c><c r="L1"><f>QUARTILE.EXC(A1:E1,5)</f><v>9</v></c><c r="M1"><f>QUARTILE.EXC(A1,1)</f><v>11</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "10").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>QUARTILE.EXC(A1:E1,1)</f><v>12.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE.EXC(A1:E1,1.9)</f><v>12.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE.EXC(A1:E1,2)</f><v>25</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE.EXC(A1:E1,3)</f><v>37.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE.EXC(A1:E1,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE.EXC(A1:E1,4)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE.EXC(A1:E1,5)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE.EXC(A1,1)</f><v>11</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_averageif() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>8</v></c><c r="C1"><v>3</v></c><c r="D1"><f>AVERAGEIF(A1:C1,"&gt;2")</f><v>0</v></c><c r="E1"><f>AVERAGEIF(A1:C1,8)</f><v>0</v></c><c r="F1"><f>AVERAGEIF(A1:C1,"&lt;0")</f><v>7</v></c><c r="G1"><f>AVERAGEIF(A1:C1,"ab")</f><v>8</v></c><c r="H1"><f>AVERAGEIF(A1:C1,"&gt;2",A1)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>AVERAGEIF(A1:C1,"&gt;2")</f><v>5.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>AVERAGEIF(A1:C1,8)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>AVERAGEIF(A1:C1,"&lt;0")</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>AVERAGEIF(A1:C1,"ab")</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>AVERAGEIF(A1:C1,"&gt;2",A1)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_sumproduct() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>8</v></c><c r="C1" t="inlineStr"><is><t>ab</t></is></c><c r="D1"><v>3</v></c><c r="E1"><v>4</v></c><c r="F1"><f>SUMPRODUCT(A1:C1)</f><v>0</v></c><c r="G1"><f>SUMPRODUCT(A1:B1,D1:E1)</f><v>0</v></c><c r="H1"><f>SUMPRODUCT(A1:C1,D1:E1)</f><v>7</v></c><c r="I1"><f>SUMPRODUCT()</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SUMPRODUCT(A1:C1)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMPRODUCT(A1:B1,D1:E1)</f><v>38</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMPRODUCT(A1:C1,D1:E1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>SUMPRODUCT()</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_minifs_and_maxifs() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>8</v></c><c r="C1"><v>3</v></c><c r="D1"><v>10</v></c><c r="E1"><v>20</v></c><c r="F1"><v>30</v></c><c r="G1"><f>MINIFS(D1:F1,A1:C1,"&gt;2")</f><v>0</v></c><c r="H1"><f>MAXIFS(D1:F1,A1:C1,"&gt;2")</f><v>0</v></c><c r="I1"><f>MINIFS(D1:F1,A1:C1,"&lt;0")</f><v>0</v></c><c r="J1"><f>MINIFS(D1:E1,A1:C1,"&gt;2")</f><v>7</v></c><c r="K1"><f>MINIFS(D1:F1,A1:C1,"ab")</f><v>8</v></c><c r="L1"><f>MINIFS(D1:F1,A1:C1,"&gt;2",A1:C1,1)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>MINIFS(D1:F1,A1:C1,"&gt;2")</f><v>20</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MAXIFS(D1:F1,A1:C1,"&gt;2")</f><v>30</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MINIFS(D1:F1,A1:C1,"&lt;0")</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MINIFS(D1:E1,A1:C1,"&gt;2")</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MINIFS(D1:F1,A1:C1,"ab")</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MINIFS(D1:F1,A1:C1,"&gt;2",A1:C1,1)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_sumifs() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>8</v></c><c r="C1"><v>3</v></c><c r="D1"><v>10</v></c><c r="E1"><v>20</v></c><c r="F1"><v>30</v></c><c r="G1"><f>SUMIFS(D1:F1,A1:C1,"&gt;2")</f><v>0</v></c><c r="H1"><f>SUMIFS(D1:F1,A1:C1,"&lt;0")</f><v>0</v></c><c r="I1"><f>SUMIFS(D1:E1,A1:C1,"&gt;2")</f><v>7</v></c><c r="J1"><f>SUMIFS(D1:F1,A1:C1,"ab")</f><v>8</v></c><c r="K1"><f>SUMIFS(D1:F1,A1:C1,"&gt;2",A1:C1,1)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SUMIFS(D1:F1,A1:C1,"&gt;2")</f><v>50</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMIFS(D1:F1,A1:C1,"&lt;0")</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMIFS(D1:E1,A1:C1,"&gt;2")</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMIFS(D1:F1,A1:C1,"ab")</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMIFS(D1:F1,A1:C1,"&gt;2",A1:C1,1)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_averageifs() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>8</v></c><c r="C1"><v>3</v></c><c r="D1"><v>10</v></c><c r="E1"><v>20</v></c><c r="F1"><v>30</v></c><c r="G1"><f>AVERAGEIFS(D1:F1,A1:C1,"&gt;2")</f><v>0</v></c><c r="H1"><f>AVERAGEIFS(D1:F1,A1:C1,"&lt;0")</f><v>4</v></c><c r="I1"><f>AVERAGEIFS(D1:E1,A1:C1,"&gt;2")</f><v>7</v></c><c r="J1"><f>AVERAGEIFS(D1:F1,A1:C1,"ab")</f><v>8</v></c><c r="K1"><f>AVERAGEIFS(D1:F1,A1:C1,"&gt;2",A1:C1,1)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>AVERAGEIFS(D1:F1,A1:C1,"&gt;2")</f><v>25</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>AVERAGEIFS(D1:F1,A1:C1,"&lt;0")</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>AVERAGEIFS(D1:E1,A1:C1,"&gt;2")</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>AVERAGEIFS(D1:F1,A1:C1,"ab")</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>AVERAGEIFS(D1:F1,A1:C1,"&gt;2",A1:C1,1)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_sumsq() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>8</v></c><c r="C1"><v>3</v></c><c r="D1" t="inlineStr"><is><t>xy</t></is></c><c r="E1"><v>1e200</v></c><c r="F1"><f>SUMSQ(A1:D1)</f><v>0</v></c><c r="G1"><f>SUMSQ()</f><v>5</v></c><c r="H1"><f>SUMSQ(E1)</f><v>6</v></c><c r="I1"><f>SUMSQ("ab")</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>SUMSQ(A1:D1)</f><v>77</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SUMSQ()</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SUMSQ(E1)</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SUMSQ("ab")</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_stdev() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>4</v></c><c r="C1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><f>STDEV.S(A1:C1)</f><v>0</v></c><c r="E1"><f>STDEV(A1:B1)</f><v>0</v></c><c r="F1"><f>STDEV.P(A1:B1)</f><v>0</v></c><c r="G1"><f>STDEVP(A1:B1)</f><v>0</v></c><c r="H1"><f>STDEV.S(A1)</f><v>5</v></c><c r="I1"><f>STDEV.P(A1)</f><v>6</v></c><c r="J1"><f>STDEV.S()</f><v>7</v></c><c r="K1"><f>STDEV.S("ab")</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>STDEV.S(A1:C1)</f><v>1.41421356</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>STDEV(A1:B1)</f><v>1.41421356</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>STDEV.P(A1:B1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>STDEVP(A1:B1)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>STDEV.S(A1)</f><v>5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>STDEV.P(A1)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>STDEV.S()</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>STDEV.S("ab")</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_var() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>4</v></c><c r="C1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><f>VAR.S(A1:C1)</f><v>0</v></c><c r="E1"><f>VAR(A1:B1)</f><v>0</v></c><c r="F1"><f>VAR.P(A1:B1)</f><v>0</v></c><c r="G1"><f>VARP(A1:B1)</f><v>0</v></c><c r="H1"><f>VAR.S(A1)</f><v>5</v></c><c r="I1"><f>VAR.P(A1)</f><v>6</v></c><c r="J1"><f>VAR.S()</f><v>7</v></c><c r="K1"><f>VAR.S("ab")</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>VAR.S(A1:C1)</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>VAR(A1:B1)</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>VAR.P(A1:B1)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>VARP(A1:B1)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>VAR.S(A1)</f><v>5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>VAR.P(A1)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>VAR.S()</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>VAR.S("ab")</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_avedev() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>4</v></c><c r="C1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><f>AVEDEV(A1:C1)</f><v>0</v></c><c r="E1"><f>AVEDEV(A1)</f><v>9</v></c><c r="F1"><f>AVEDEV()</f><v>7</v></c><c r="G1"><f>AVEDEV("ab")</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>AVEDEV(A1:C1)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>AVEDEV(A1)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>AVEDEV()</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>AVEDEV("ab")</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_devsq() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>4</v></c><c r="C1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><f>DEVSQ(A1:C1)</f><v>0</v></c><c r="E1"><f>DEVSQ(A1)</f><v>9</v></c><c r="F1"><f>DEVSQ()</f><v>7</v></c><c r="G1"><f>DEVSQ("ab")</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>DEVSQ(A1:C1)</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>DEVSQ(A1)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>DEVSQ()</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>DEVSQ("ab")</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_geomean() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>8</v></c><c r="C1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><v>0</v></c><c r="E1"><v>-3</v></c><c r="F1"><f>GEOMEAN(A1:C1)</f><v>0</v></c><c r="G1"><f>GEOMEAN(A1)</f><v>0</v></c><c r="H1"><f>GEOMEAN(D1)</f><v>5</v></c><c r="I1"><f>GEOMEAN(A1,E1)</f><v>6</v></c><c r="J1"><f>GEOMEAN()</f><v>7</v></c><c r="K1"><f>GEOMEAN("ab")</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>GEOMEAN(A1:C1)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>GEOMEAN(A1)</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GEOMEAN(D1)</f><v>5</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>GEOMEAN(A1,E1)</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>GEOMEAN()</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GEOMEAN("ab")</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_harmean() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>8</v></c><c r="C1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><v>0</v></c><c r="E1"><v>-3</v></c><c r="L1"><v>1e-320</v></c><c r="F1"><f>HARMEAN(A1:C1)</f><v>0</v></c><c r="G1"><f>HARMEAN(A1)</f><v>0</v></c><c r="H1"><f>HARMEAN(D1)</f><v>5</v></c><c r="I1"><f>HARMEAN(A1,E1)</f><v>6</v></c><c r="J1"><f>HARMEAN()</f><v>7</v></c><c r="K1"><f>HARMEAN("ab")</f><v>8</v></c><c r="M1"><f>HARMEAN(L1)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>HARMEAN(A1:C1)</f><v>3.2</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>HARMEAN(A1)</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>HARMEAN(D1)</f><v>5</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>HARMEAN(A1,E1)</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>HARMEAN()</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>HARMEAN("ab")</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>HARMEAN(L1)</f><v>9</v>"#), "{sheet}");
    }

    #[tokio::test]
    async fn edit_cell_writes_the_workbook_and_refuses_a_formula() {
        let drawing = "<drawing>keep-me</drawing>";
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/sharedStrings.xml",
                r#"<sst><si><t>Orchid</t></si></sst>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1"><v>42</v></c><c r="C1"><f>1+1</f><v>2</v></c></row></sheetData></worksheet>"#,
            ),
            ("xl/drawings/drawing1.xml", drawing),
        ]);
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("book.xlsx");
        std::fs::write(&file, bytes).expect("write");
        let fs_path = orchid_fs::FsPath::from_local(&file).expect("fs path");
        let registry = Arc::new(orchid_fs::FsProviderRegistry::new());
        registry
            .register(Arc::new(orchid_fs::LocalProvider::new()))
            .expect("register local");
        let mut viewer = OfficeViewer::new();
        viewer
            .open(fs_path, Arc::clone(&registry))
            .await
            .expect("open");
        let err = viewer
            .edit_cell(Arc::clone(&registry), "Budgets", "C1", "9")
            .await
            .expect_err("formula");
        assert_eq!(err.to_string(), "viewer-sheet-formula");
        viewer
            .edit_cell(Arc::clone(&registry), "Budgets", "B1", "7")
            .await
            .expect("save");
        let first = viewer.snapshot();
        let second = viewer.snapshot();
        match (&first, &second) {
            (ViewerSnapshot::Sheet(a), ViewerSnapshot::Sheet(b)) => {
                assert!(Arc::ptr_eq(&a.sheets, &b.sheets));
                assert_eq!(a.sheets[0].rows[0][0].text, "Orchid");
                assert_eq!(a.sheets[0].rows[0][1].text, "7");
                assert_eq!(a.sheets[0].rows[0][2].text, "2");
            }
            other => panic!("expected a sheet snapshot, got {other:?}"),
        }
        let on_disk = std::fs::read(&file).expect("reread");
        let preview = render_office(&on_disk, false).expect("preview");
        let OfficePreview::Sheets(book) = preview else {
            panic!("workbook should be a sheet table");
        };
        assert_eq!(book.sheets[0].rows[0][1].text, "7");
        let mut archive = ZipArchive::new(Cursor::new(on_disk)).unwrap();
        let kept = read_entry(&mut archive, "xl/drawings/drawing1.xml").unwrap();
        assert_eq!(kept, drawing);
        assert!(!dir.path().join("book.xlsx.orchid-save").exists());
    }

    #[test]
    fn slides_html_keeps_order_and_notes() {
        let bytes = zip_bytes(&[
            (
                "ppt/slides/slide2.xml",
                r#"<p:sld><p:cSld><p:spTree><a:p><a:t>Second</a:t></a:p></p:spTree></p:cSld></p:sld>"#,
            ),
            (
                "ppt/slides/slide1.xml",
                r#"<p:sld><p:cSld><p:spTree><a:p><a:t>Hello orchid</a:t></a:p></p:spTree></p:cSld></p:sld>"#,
            ),
            (
                "ppt/notesSlides/notesSlide1.xml",
                r#"<p:notes><a:p><a:t>Speaker note</a:t></a:p></p:notes>"#,
            ),
        ]);
        let preview = render_office(&bytes, true).unwrap();
        let OfficePreview::Slides(preview) = preview else {
            panic!("slides should stay an HTML preview");
        };
        let hello = preview.html.find("Hello orchid").unwrap();
        let second = preview.html.find("Second").unwrap();
        assert!(hello < second, "{}", preview.html);
        assert!(preview.html.contains("Speaker note"), "{}", preview.html);
        assert!(preview.info.contains("2 slides"), "{}", preview.info);
    }
}
