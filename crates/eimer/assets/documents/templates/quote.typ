// Quote (Angebot). Data: examples/quote.yaml.
#import "../lib/doc.typ": *

#let data = yaml(sys.inputs.data)
#let sender = yaml(sys.inputs.sender)
#let vat = vat-setup(sender, data)
#let sums = totals(data.items, vat.rate)
#let valid-days = data.at("valid_days", default: 30)

#let info = (
  ([Angebot Nr.], text(weight: "bold", data.number)),
  ([Datum], fmt-date(data.date)),
  ([Gültig bis], fmt-date(add-days(data.date, valid-days))),
  ..if data.at("project", default: none) != none { (([Projekt], data.project),) },
)

#show: letter.with(sender, recipient: data.client, info: info, title: data.at("title", default: "Angebot"), logo-variant: data.at("logo", default: none))

#markup(data.at("intro", default: none))

#items-table(data.items)
#totals-block(sums, vat.rate, vat-label: vat.label)

#v(6mm)
#if vat.note != none { text(size: 9pt, fill: muted, vat.note); parbreak() }
#markup(data.at("terms", default: none))

#markup(data.at("outro", default: none))
#greeting(sender)
