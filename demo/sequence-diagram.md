# Mermaid Sequence Diagram

Sequence diagrams are rendered natively by mdterm (lifelines, messages,
activations and notes drawn with box-drawing characters), so they stay crisp
and readable in every terminal — including the half-block fallback over SSH.

```mermaid
sequenceDiagram
    autonumber
    participant U as User
    participant W as Web App
    participant A as Auth Service
    participant DB as Database
    participant P as Payment

    U->>W: Enter credentials
    W->>A: POST /login
    A->>DB: Verify user
    DB-->>A: user record
    A-->>W: 200 OK + JWT
    W-->>U: Show dashboard

    Note over U,W: Session established

    U->>W: Place order
    W->>A: Validate JWT
    A-->>W: valid
    W->>P: Charge card
    P-->>W: receipt
    W->>DB: Save order
    W-->>U: Order confirmed
```
