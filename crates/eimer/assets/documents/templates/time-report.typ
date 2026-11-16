// Time report (Zeitbuchungen), A4 landscape. Data: examples/time-report.yaml.
// Entries: date, location (onsite | remote), description (markup, one line per task), hours,
// billed (default true; false lists the entry but keeps it out of the billed totals).
#import "../lib/doc.typ": *

#let data = yaml(sys.inputs.data)
#let sender = yaml(sys.inputs.sender)
#let entries = data.entries.sorted(key: e => str(e.date))
#let sum-of(list) = list.map(e => dec(e.hours)).fold(decimal("0"), (a, b) => a + b)
#let is-billed(e) = e.at("billed", default: true)
#let billed = entries.filter(is-billed)
#let onsite = sum-of(billed.filter(e => e.at("location", default: "remote") == "onsite"))
#let remote = sum-of(billed.filter(e => e.at("location", default: "remote") != "onsite"))
#let unbilled = sum-of(entries.filter(e => not is-billed(e)))
#let total = onsite + remote
#let all-hours = total + unbilled
#let has-unbilled = unbilled > decimal("0")
#let unbilled-fill = rule-color

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
  columns: (1fr, if has-unbilled { 118mm } else { 92mm }),
  align: (left + bottom, left + bottom),
  [
    #heading-display(size: 26pt, data.at("title", default: "Zeitbuchungen"))
    #v(-3mm)
    #text(size: 11pt, fill: muted)[#fmt-date(data.period.from) – #fmt-date(data.period.to) · Kunde: #data.client#if data.at("project", default: none) not in (none, data.client) [ · #data.project]]
  ],
  [
    #let parts = ((onsite, hitze), (remote, kobalt), (unbilled, unbilled-fill)).filter(p => p.at(0) > decimal("0"))
    #grid(
      columns: parts.map(p => float(p.at(0) / all-hours) * 1fr),
      column-gutter: 1mm,
      ..parts.map(p => box(height: 2.2mm, width: 100%, radius: 2mm, fill: p.at(1))),
    )
    #v(1mm)
    #set text(size: 8pt, fill: muted)
    #let figure(hours, label) = box[#text(weight: "bold", fill: tinte)[#num(hours) Std.] #label]
    #stack(
      dir: ltr,
      spacing: 1fr,
      figure(onsite, [vor Ort]),
      figure(remote, [remote]),
      ..if has-unbilled { (figure(unbilled, [nicht abgerechnet]),) },
      figure(total, if has-unbilled [abgerechnet] else [gesamt]),
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
      if is-billed(e) {
        text(weight: "bold")[#num(e.hours) Std.]
      } else {
        set text(fill: muted)
        stack(dir: ttb, spacing: 1.2mm, strike[#num(e.hours) Std.], text(size: 7pt)[nicht abgerechnet])
      },
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
    if has-unbilled { text(fill: muted)[Abgerechnet] } else { [] },
    text(font: display, weight: "bold", size: 15pt)[#num(total) Std.],
    ..if has-unbilled {
      (text(fill: muted)[Nicht abgerechnet], text(fill: muted, weight: "bold")[#num(unbilled) Std.])
    },
  )
])
