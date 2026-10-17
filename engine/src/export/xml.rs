//! A tiny indented XML writer.

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Control characters are not allowed in XML 1.0.
            c if (c as u32) < 0x20 && c != '\n' && c != '\t' && c != '\r' => {}
            c => out.push(c),
        }
    }
    out
}

pub struct Xml {
    pub out: String,
    depth: usize,
    stack: Vec<String>,
}

impl Xml {
    pub fn new(header: &str) -> Xml {
        Xml {
            out: header.to_string(),
            depth: 0,
            stack: Vec::new(),
        }
    }

    fn indent(&mut self) {
        for _ in 0..self.depth {
            self.out.push_str("    ");
        }
    }

    fn attrs(&mut self, attrs: &[(&str, String)]) {
        for (k, v) in attrs {
            self.out.push(' ');
            self.out.push_str(k);
            self.out.push_str("=\"");
            self.out.push_str(&escape(v));
            self.out.push('"');
        }
    }

    pub fn open(&mut self, tag: &str, attrs: &[(&str, String)]) {
        self.indent();
        self.out.push('<');
        self.out.push_str(tag);
        self.attrs(attrs);
        self.out.push_str(">\n");
        self.depth += 1;
        self.stack.push(tag.to_string());
    }

    pub fn close(&mut self) {
        let tag = self.stack.pop().expect("balanced xml");
        self.depth -= 1;
        self.indent();
        self.out.push_str("</");
        self.out.push_str(&tag);
        self.out.push_str(">\n");
    }

    pub fn empty(&mut self, tag: &str, attrs: &[(&str, String)]) {
        self.indent();
        self.out.push('<');
        self.out.push_str(tag);
        self.attrs(attrs);
        self.out.push_str("/>\n");
    }

    pub fn text(&mut self, tag: &str, text: impl ToString) {
        self.indent();
        self.out.push('<');
        self.out.push_str(tag);
        self.out.push('>');
        self.out.push_str(&escape(&text.to_string()));
        self.out.push_str("</");
        self.out.push_str(tag);
        self.out.push_str(">\n");
    }

    pub fn finish(self) -> String {
        assert!(self.stack.is_empty(), "unclosed xml elements");
        self.out
    }
}
