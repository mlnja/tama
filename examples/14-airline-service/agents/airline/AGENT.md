---
name: airline
description: Airline customer service — routes requests via FSM to specialist agents. Ported from OpenAI Swarm.
version: 1.0.0
pattern: fsm
initial: triage
states:
  triage:
    - refunds: refunds
    - baggage: baggage
    - booking: booking
  refunds:
    - done: ~
    - escalate: triage
  baggage: ~
  booking: ~
---
