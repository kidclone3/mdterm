# Mermaid ER Diagram

`erDiagram` is rendered natively by mdterm as ASCII art — entity cards with
PK/FK badges and crow's-foot cardinality endpoints are drawn with
box-drawing characters, so the diagram stays crisp in every terminal
including the half-block fallback over SSH.

```mermaid
erDiagram
    CUSTOMER ||--o{ ORDER : places
    ORDER ||--|{ LINE_ITEM : contains
    PRODUCT ||--o{ LINE_ITEM : "appears in"
    CUSTOMER ||--o{ REVIEW : writes

    CUSTOMER {
        bigint id PK
        string email UK
        string name
        timestamp created_at
    }
    ORDER {
        bigint id PK
        bigint customer_id FK
        timestamp placed_at
        string status
    }
    LINE_ITEM {
        bigint id PK
        bigint order_id FK
        bigint product_id FK
        int quantity
        decimal unit_price
    }
    PRODUCT {
        bigint id PK
        string name
        decimal price
        int stock
    }
    REVIEW {
        bigint id PK
        bigint customer_id FK
        bigint product_id FK
        int rating
        string body
    }
```
