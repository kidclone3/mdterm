# Mermaid State Diagram (v2)

`stateDiagram-v2` is rendered natively by mdterm as ASCII art — states,
transitions, initial/final pseudo-states, fork/join bars, notes, and
composite (nested) states are drawn with box-drawing characters, so the
diagram stays crisp in every terminal including the half-block fallback
over SSH.

```mermaid
stateDiagram-v2
    [*] --> Created : new order
    Created --> Paid : payment_ok
    Created --> Cancelled : user_cancel

    Paid --> Packed : warehouse_pick
    Packed --> Shipped : label_printed
    Shipped --> Delivered : carrier_dropoff

    Delivered --> Closed : confirm
    Closed --> [*]
    Cancelled --> [*]

    note right of Paid
        Funds captured
        by the gateway
    end note

    state Cancelled {
        [*] --> Refunded
        Refunded --> [*]
    }
```
