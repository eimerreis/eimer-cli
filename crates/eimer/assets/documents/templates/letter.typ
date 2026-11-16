// Plain letter on the letterhead. Data: examples/letter.yaml.
#import "../lib/doc.typ": *

#let data = yaml(sys.inputs.data)
#let sender = yaml(sys.inputs.sender)

#let info = (
  ([Datum], fmt-date(data.date)),
  ..data.at("info", default: (:)).pairs().map(((label, value)) => ([#label], [#value])),
)

#show: letter.with(sender, recipient: data.recipient, info: info, title: data.at("subject", default: none), logo-variant: data.at("logo", default: none))

#markup(data.body)
#greeting(sender, closing: data.at("closing", default: "Viele Grüße"))
