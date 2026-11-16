// Time report (Zeitbuchungen), A4 landscape. Data: examples/time-report.yaml.
// Entries: date, location (onsite | remote), description (markup, one line per task), hours.
#import "../lib/doc.typ": *

#let data = yaml(sys.inputs.data)
#let sender = yaml(sys.inputs.sender)
#let entries = data.entries.sorted(key: e => str(e.date))
#let sum-of(list) = list.map(e => dec(e.hours)).fold(decimal("0"), (a, b) => a + b)
#let onsite = sum-of(entries.filter(e => e.at("location", default: "remote") == "onsite"))
#let remote = sum-of(entries.filter(e => e.at("location", default: "remote") != "onsite"))
#let total = onsite + remote

#let pill(location) = {
  let on = location == "onsite"
  box(
    fill: if on { hitze } else { kobalt.lighten(85%) },
    radius: 4pt,
    inset: (x: 2.2mm, y: 1mm),
    text(size: 7.5pt, weight: "bold", fill: if on { tinte } else { kobalt }, if on [Vor Ort] else [Remote]),
  )
}

#let footer-line = {
  line(length: 100%, stroke: 0.5pt + rule-color)
  v(1mm)
  set text(size: 7.5pt, fill: muted)
  grid(
    columns: (1fr, auto),
    [#text(weight: "bold", fill: tinte, sender.company) · Inh. #sender.owner · #sender.street, #sender.zip #sender.city · #sender.email · #sender.web],
    context [Seite #counter(page).display() von #counter(page).final().first()],
  )
}

#show: base.with(
  sender,
  title: "Zeitbuchungen " + data.client,
  flipped: true,
  margin: (x: 18mm, top: 14mm, bottom: 22mm),
  footer: footer-line,
  background: context if counter(page).get().first() == 1 {
    place(top + right, dx: 12mm, dy: -16mm, image("../brand/mark-hitze.svg", width: 40mm))
  },
)

#logo(sender, variant: data.at("logo", default: none), height: 11mm)
#v(5mm)
#grid(
  columns: (1fr, 92mm),
  align: (left + bottom, left + bottom),
  [
    #heading-display(size: 26pt, data.at("title", default: "Zeitbuchungen"))
    #v(-3mm)
    #text(size: 11pt, fill: muted)[#fmt-date(data.period.from) – #fmt-date(data.period.to) · Kunde: #data.client#if data.at("project", default: none) not in (none, data.client) [ · #data.project]]
  ],
  [
    #let share = if total == decimal("0") { 0 } else { float(onsite / total) }
    #grid(
      columns: (calc.max(share, 0.02) * 1fr, calc.max(1 - share, 0.02) * 1fr),
      column-gutter: 1mm,
      box(height: 2.2mm, width: 100%, radius: 2mm, fill: hitze),
      box(height: 2.2mm, width: 100%, radius: 2mm, fill: kobalt),
    )
    #v(1mm)
    #set text(size: 8pt, fill: muted)
    #grid(
      columns: (1fr, 1fr, auto),
      [#text(weight: "bold", fill: tinte)[#num(onsite) Std.] vor Ort],
      align(center)[#text(weight: "bold", fill: tinte)[#num(remote) Std.] remote],
      [#text(weight: "bold", fill: tinte)[#num(total) Std.] gesamt],
    )
  ],
)
#v(6mm)

#set text(size: 9.5pt)
#table(
  columns: (28mm, 24mm, 1fr, auto),
  column-gutter: 4mm,
  inset: (x: 0pt, y: 2.2mm),
  stroke: none,
  align: (left, left, left, right),
  table.header(
    ..([Datum], [Ort], [Beschreibung], [Dauer]).map(cell => text(weight: "bold", size: 8.5pt, cell)),
  ),
  table.hline(stroke: 0.8pt + tinte),
  ..entries
    .map(e => (
      fmt-date(e.date),
      pill(e.at("location", default: "remote")),
      {
        set par(leading: 0.55em)
        str(e.description).trim().split("\n").map(line => markup(line)).join(linebreak())
      },
      text(weight: "bold")[#num(e.hours) Std.],
      hairline,
    ))
    .flatten(),
)

#v(-1mm)
#align(right, block(width: 75mm, breakable: false)[
  #set text(size: 9.5pt)
  #grid(
    columns: (1fr, auto),
    row-gutter: 2.4mm,
    align: (left + horizon, right + horizon),
    text(fill: muted)[Summe vor Ort], text(weight: "bold")[#num(onsite) Std.],
    text(fill: muted)[Summe remote], text(weight: "bold")[#num(remote) Std.],
    grid.cell(colspan: 2, line(length: 100%, stroke: 0.8pt + tinte)),
    [], text(font: display, weight: "bold", size: 15pt)[#num(total) Std.],
  )
])
