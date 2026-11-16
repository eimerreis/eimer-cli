// Payment reminder (Zahlungserinnerung / Mahnung). Data: examples/reminder.yaml.
#import "../lib/doc.typ": *

#let data = yaml(sys.inputs.data)
#let sender = yaml(sys.inputs.sender)
#let level = data.at("level", default: 0)
#let title = if level == 0 { "Zahlungserinnerung" } else { str(level) + ". Mahnung" }
#let open-amount = dec(data.invoice.amount) - dec(data.invoice.at("paid", default: 0))
#let fee = dec(data.at("fee", default: 0))
#let pay-days = data.at("pay_days", default: 10)

#let info = (
  ([Datum], fmt-date(data.date)),
  ([Rechnung Nr.], text(weight: "bold", data.invoice.number)),
)

#show: letter.with(sender, recipient: data.client, info: info, title: title, logo-variant: data.at("logo", default: none))

#markup(data.at("intro", default: none))

#block(width: 100%, inset: (y: 3mm))[
  #set text(size: 9.5pt)
  #grid(
    columns: (1fr, auto),
    row-gutter: 2.6mm,
    align: (left + horizon, right + horizon),
    text(fill: muted)[Rechnung #data.invoice.number vom #fmt-date(data.invoice.date)], money(data.invoice.amount),
    ..if dec(data.invoice.at("paid", default: 0)) != decimal("0") { (text(fill: muted)[Bereits bezahlt], money(-dec(data.invoice.paid))) },
    text(fill: muted)[Fällig seit], fmt-date(data.invoice.due),
    ..if fee != decimal("0") { (text(fill: muted)[Mahngebühr], money(fee)) },
    grid.cell(colspan: 2, line(length: 100%, stroke: 0.8pt + tinte)),
    text(weight: "bold")[Offen], text(font: display, weight: "bold", size: 13pt, money(open-amount + fee)),
  )
]

Bitte überweise den offenen Betrag bis zum *#fmt-date(add-days(data.date, pay-days))*#if sender.bank.at("iban", default: "") != "" [ auf das Konto #iban(sender.bank.iban)]. Falls sich deine Zahlung mit diesem Schreiben überschnitten hat, betrachte es bitte als gegenstandslos.

#markup(data.at("outro", default: none))
#greeting(sender)
