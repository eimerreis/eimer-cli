// Contract (Vertrag). The clauses are Typst markup in `body`: "= Title" starts a numbered § section.
// Data: examples/contract.yaml.
#import "../lib/doc.typ": *

#let data = yaml(sys.inputs.data)
#let sender = yaml(sys.inputs.sender)

#let party(p, role) = block(breakable: false)[
  #text(size: 8pt, fill: muted, upper(role)) \
  #text(weight: "bold", p.name) \
  #if p.at("represented_by", default: none) != none [vertreten durch #p.represented_by \ ]
  #p.street, #p.zip #p.city
]

#let contractor = (
  name: sender.company + ", Inh. " + sender.owner,
  street: sender.street,
  zip: sender.zip,
  city: sender.city,
)

#let signature(role, name) = block(breakable: false, width: 100%)[
  #v(16mm)
  #line(length: 100%, stroke: 0.6pt + tinte)
  #set text(size: 8.5pt)
  #grid(columns: (1fr, auto), [Ort, Datum], [])
  #v(-2mm)
  #text(weight: "bold", role) · #name
]

#show: base.with(sender, title: data.title, background: letter-background)

#set heading(numbering: (..n) => "§ " + str(n.pos().first()))
#show heading.where(level: 1): it => block(above: 7mm, below: 3mm, sticky: true)[
  #text(font: display, weight: "bold", size: 11pt)[#counter(heading).display() #h(2mm) #it.body]
]
#set enum(numbering: "(1)", indent: 0pt, body-indent: 3mm)
#set par(justify: true)

#logo(sender, variant: data.at("logo", default: none))
#v(14mm)
#heading-display(size: 24pt, data.title)
#if data.at("subtitle", default: none) != none {
  v(-2mm)
  text(size: 11pt, fill: muted, data.subtitle)
}
#v(8mm)
#grid(
  columns: (1fr, 1fr),
  column-gutter: 8mm,
  party(data.client, data.at("client_role", default: "Auftraggeber")),
  party(contractor, data.at("contractor_role", default: "Auftragnehmer")),
)
#v(4mm)
#line(length: 100%, stroke: 0.5pt + rule-color)

#markup(data.body)

#v(6mm)
#grid(
  columns: (1fr, 1fr),
  column-gutter: 12mm,
  signature(data.at("client_role", default: "Auftraggeber"), data.client.at("signatory", default: data.client.name)),
  signature(data.at("contractor_role", default: "Auftragnehmer"), sender.owner),
)
