// Shared building blocks for EimerReis documents: brand tokens, number/date formatting,
// the DIN 5008 (form B) letter page, line items and totals.

// ── Brand ────────────────────────────────────────────────────────────────────
#let kobalt = rgb("#164BFF")
#let hitze = rgb("#FF6A2B")
#let papier = rgb("#F7F3E8")
#let tinte = rgb("#111111")
#let muted = rgb("#5E584D")
#let rule-color = rgb("#DDD5C3")
#let sheet = rgb("#FDFBF6")

#let display = "Unbounded"
#let body-font = "Figtree"

// ── Values ───────────────────────────────────────────────────────────────────
/// Exact decimal from a YAML number or string ("12,5" and "12.5" both work).
#let dec(x) = if type(x) == decimal { x } else { decimal(str(x).replace(",", ".")) }

/// Thousands separators for a non-negative integer: 1234567 → "1.234.567".
#let group(n) = {
  let digits = str(n).clusters()
  let out = ""
  for (i, digit) in digits.enumerate() {
    if i > 0 and calc.rem(digits.len() - i, 3) == 0 { out += "." }
    out += digit
  }
  out
}

#let cents-of(x) = int(calc.round(dec(x) * 100))

/// German money format: 1234.5 → "1.234,50 €".
#let money(x, currency: "€") = {
  let cents = cents-of(x)
  let a = calc.abs(cents)
  let rest = calc.rem(a, 100)
  let sign = if cents < 0 { "−" } else { "" }
  sign + group(calc.quo(a, 100)) + "," + (if rest < 10 { "0" } else { "" }) + str(rest) + " " + currency
}

/// German number without trailing zeros: 24.5 → "24,5", 2 → "2", 0.25 → "0,25".
#let num(x) = {
  let cents = cents-of(x)
  let a = calc.abs(cents)
  let rest = calc.rem(a, 100)
  let out = (if cents < 0 { "−" } else { "" }) + group(calc.quo(a, 100))
  if rest != 0 {
    let frac = (if rest < 10 { "0" } else { "" }) + str(rest)
    out += "," + frac.trim("0", at: end)
  }
  out
}

#let round2(x) = calc.round(x, digits: 2)

/// "2026-11-16" → datetime.
#let parse-date(s) = {
  let parts = str(s).split("-")
  datetime(year: int(parts.at(0)), month: int(parts.at(1)), day: int(parts.at(2)))
}

/// "2026-11-16" (or a datetime) → "16.11.2026".
#let fmt-date(s) = {
  let d = if type(s) == datetime { s } else { parse-date(s) }
  d.display("[day].[month].[year]")
}

#let add-days(s, days) = parse-date(s) + duration(days: days)

/// YAML text fields are Typst markup: *bold*, _italic_, lists with "-", blank line = paragraph.
/// IBAN in groups of four with non-breaking spaces, so it never wraps inside.
#let iban(s) = {
  let chars = str(s).replace(" ", "").clusters()
  range(0, chars.len(), step: 4).map(i => chars.slice(i, calc.min(i + 4, chars.len())).join()).join("\u{a0}")
}

#let markup(s) = if s == none or s == "" { none } else { eval(str(s), mode: "markup") }

// ── Page ─────────────────────────────────────────────────────────────────────
#let footer-columns(sender) = {
  let bank = sender.at("bank", default: (:))
  let tax = ()
  if sender.at("vat_id", default: "") != "" { tax.push[USt-IdNr. #sender.vat_id] }
  if sender.at("tax_number", default: "") != "" { tax.push[Steuernummer #sender.tax_number] }
  set text(size: 7.5pt, fill: muted)
  set par(leading: 0.5em)
  grid(
    columns: (1.15fr, 0.85fr, 1.2fr),
    column-gutter: 6mm,
    [
      #text(fill: tinte, weight: "bold")[#sender.company] \
      Inh. #sender.owner \
      #sender.street, #sender.zip #sender.city
    ],
    [
      #sender.email \
      #sender.phone \
      #sender.web
    ],
    [
      #bank.at("name", default: "") \
      #if bank.at("iban", default: "") != "" [IBAN #iban(bank.iban)] \
      #((if bank.at("bic", default: "") != "" { ([BIC #bank.bic],) } else { () }) + tax).join(" · ")
    ],
  )
}

#let page-number = context {
  let total = counter(page).final().first()
  if total > 1 {
    set text(size: 7.5pt, fill: muted)
    align(right)[Seite #counter(page).display() von #total]
  }
}

#let first-page = context counter(page).get().first() == 1

/// Fold marks (105 mm, 210 mm), hole mark (148.5 mm) and the melting mark in the top-right corner.
#let letter-background = context if counter(page).get().first() == 1 {
  for y in (105mm, 210mm) { place(top + left, dy: y, line(length: 5mm, stroke: 0.4pt + rule-color)) }
  place(top + left, dy: 148.5mm, line(length: 7mm, stroke: 0.4pt + rule-color))
  place(top + right, dx: 12mm, dy: -15mm, image("../brand/mark-hitze.svg", width: 44mm))
}

/// Lockups: `eimerreis` (mark + wordmark), `name` (mark + name in Unbounded, one line),
/// `name-stacked` (mark + name on two lines, for formal clients).
#let logo-files = (
  eimerreis: "../brand/logo.svg",
  name: "../brand/logo-name.svg",
  name-stacked: "../brand/logo-name-stacked.svg",
)

/// The document's `logo:` wins over the sender's default.
#let logo(sender, variant: none, height: 12.5mm) = {
  let choice = if variant != none { variant } else { sender.at("logo", default: "eimerreis") }
  assert(choice in logo-files, message: "Unknown logo \"" + choice + "\". Known: " + logo-files.keys().join(", "))
  image(logo-files.at(choice), height: height)
}

/// Base page and text setup shared by every template.
#let base(sender, title: "", flipped: false, background: none, footer: auto, margin: auto, body) = {
  set document(title: title, author: sender.owner)
  set text(font: body-font, size: 10pt, lang: "de", region: "DE", fill: tinte, number-type: "lining")
  set par(leading: 0.7em, spacing: 1.2em)
  set page(
    paper: "a4",
    flipped: flipped,
    fill: sheet,
    margin: if margin == auto { (left: 25mm, right: 20mm, top: 16mm, bottom: 34mm) } else { margin },
    background: background,
    footer: if footer == auto {
      line(length: 100%, stroke: 0.5pt + rule-color)
      v(2mm)
      footer-columns(sender)
      v(1mm)
      page-number
    } else { footer },
    footer-descent: 8mm,
  )
  set strong(delta: 200)
  // Keep "§ 3", "Abs. 2" and IBAN groups together.
  show "§ ": "§\u{a0}"
  show "Abs. ": "Abs.\u{a0}"
  body
}

#let heading-display(size: 17pt, body) = text(font: display, weight: "bold", size: size, tracking: -0.01em, body)

/// Label/value pairs for the information block next to the address.
#let info-block(rows) = {
  set text(size: 9pt)
  grid(
    columns: (auto, auto),
    column-gutter: 5mm,
    row-gutter: 2.2mm,
    align: left,
    ..rows.map(((label, value)) => (text(fill: muted, label), value)).flatten(),
  )
}

/// DIN 5008 form B letter: logo, sender line + address field at 45 mm, information block on the right.
/// `recipient` keys: name, attention?, street, zip, city, country?
#let letter(sender, recipient: (:), info: (), title: none, logo-variant: none, body) = {
  base(sender, title: if title == none { "" } else { title }, background: letter-background)[
    #block(height: 81mm, width: 100%)[
      #logo(sender, variant: logo-variant)
      #place(top + left, dy: 29mm, block(width: 85mm)[
        #text(size: 7pt, fill: muted)[#sender.company · #sender.street · #sender.zip #sender.city]
        #v(-1.6mm)
        #line(length: 100%, stroke: 0.5pt + rule-color)
        #v(1mm)
        #set par(leading: 0.55em)
        #text(size: 10.5pt)[
          #recipient.name \
          #if recipient.at("attention", default: none) != none [z. Hd. #recipient.attention \ ]
          #recipient.street \
          #recipient.zip #recipient.city
          #if recipient.at("country", default: none) != none [\ #recipient.country]
        ]
      ])
      #place(top + right, dy: 30mm, block(width: 68mm, info-block(info)))
    ]
    #if title != none {
      heading-display(title)
      v(4mm)
    }
    #body
  ]
}

#let greeting(sender, closing: "Viele Grüße") = {
  v(2mm)
  closing
  v(5mm)
  text(font: display, weight: "bold", size: 12pt, sender.owner)
}

// ── Line items and totals ───────────────────────────────────────────────────
#let line-total(item) = round2(dec(item.quantity) * dec(item.price))

/// Net, VAT and gross for items. `vat-rate` in percent (19, 7, 0).
#let totals(items, vat-rate) = {
  let net = items.map(line-total).fold(decimal("0"), (a, b) => a + b)
  let vat = round2(net * dec(vat-rate) / 100)
  (net: net, vat: vat, gross: net + vat)
}

#let hairline = table.hline(stroke: 0.5pt + rule-color)

#let items-table(items) = {
  set text(size: 9.5pt)
  table(
    columns: (7mm, 1fr, auto, auto, auto),
    column-gutter: 4mm,
    inset: (x: 0pt, y: 2.6mm),
    stroke: none,
    align: (left, left, right, right, right),
    table.header(
      ..([Pos.], [Leistung], [Menge], [Einzelpreis], [Gesamt]).map(cell => text(weight: "bold", size: 8.5pt, cell)),
    ),
    table.hline(stroke: 0.8pt + tinte),
    ..items
      .enumerate()
      .map(((i, item)) => (
        text(fill: muted, str(i + 1)),
        [
          #item.description
          #if item.at("detail", default: none) != none {
            linebreak()
            text(size: 8.5pt, fill: muted, markup(item.detail))
          }
        ],
        [#num(item.quantity) #item.at("unit", default: "")],
        money(item.price),
        text(weight: "bold", money(line-total(item))),
        hairline,
      ))
      .flatten(),
  )
}

#let totals-block(sums, vat-rate, vat-label: none) = {
  set text(size: 9.5pt)
  align(right, block(width: 75mm, grid(
    columns: (1fr, auto),
    row-gutter: 2.4mm,
    align: (left + horizon, right + horizon),
    text(fill: muted)[Netto], money(sums.net),
    text(fill: muted)[#if vat-label != none { vat-label } else [Umsatzsteuer #num(vat-rate) %]], money(sums.vat),
    grid.cell(colspan: 2, line(length: 100%, stroke: 0.8pt + tinte)),
    text(weight: "bold")[Gesamt], text(font: display, weight: "bold", size: 13pt, money(sums.gross)),
  )))
}

/// VAT rate and the legal note for small businesses (§ 19 UStG) or reverse charge.
#let vat-setup(sender, data) = {
  if sender.at("small_business", default: false) {
    (rate: 0, label: [Umsatzsteuer], note: [Gemäß § 19 UStG wird keine Umsatzsteuer berechnet.])
  } else if data.at("reverse_charge", default: false) {
    (rate: 0, label: [Umsatzsteuer], note: [Steuerschuldnerschaft des Leistungsempfängers (Reverse Charge).])
  } else {
    (rate: data.at("vat_rate", default: 19), label: none, note: none)
  }
}

#let service-period(data) = {
  let p = data.at("service_period", default: none)
  if p == none { return none }
  if type(p) != dictionary { return fmt-date(p) }
  let (from, to) = (parse-date(p.from), parse-date(p.to))
  if from.year() == to.year() [#from.display("[day].[month].") – #fmt-date(to)] else [#fmt-date(from) – #fmt-date(to)]
}
