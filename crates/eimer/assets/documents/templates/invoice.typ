// Invoice (Rechnung). Data: examples/invoice.yaml.
#import "../lib/doc.typ": *

#let data = yaml(sys.inputs.data)
#let sender = yaml(sys.inputs.sender)
#let vat = vat-setup(sender, data)
#let sums = totals(data.items, vat.rate)
#let due-days = data.at("due_days", default: 14)

#let info = (
  ([Rechnung Nr.], text(weight: "bold", data.number)),
  ([Datum], fmt-date(data.date)),
  ..if service-period(data) != none { (([Leistungszeitraum], service-period(data)),) },
  ..if data.at("customer_number", default: none) != none { (([Kundennummer], data.customer_number),) },
  ..if data.at("project", default: none) != none { (([Projekt], data.project),) },
)

#show: letter.with(sender, recipient: data.client, info: info, title: data.at("title", default: "Rechnung"), logo-variant: data.at("logo", default: none))

#markup(data.at("intro", default: none))

#items-table(data.items)
#totals-block(sums, vat.rate, vat-label: vat.label)

#v(6mm)
#if vat.note != none { text(size: 9pt, fill: muted, vat.note); parbreak() }
Bitte überweise *#money(sums.gross)* bis zum *#fmt-date(add-days(data.date, due-days))* unter Angabe der Rechnungsnummer *#data.number*#if sender.bank.at("iban", default: "") != "" [ auf das Konto #iban(sender.bank.iban)].

#markup(data.at("outro", default: none))
#greeting(sender)
