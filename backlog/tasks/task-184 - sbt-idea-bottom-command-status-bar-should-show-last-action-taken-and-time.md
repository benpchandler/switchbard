---
id: TASK-184
title: 'sbt idea: bottom command / status bar should show last action taken and time'
status: To Do
assignee: []
created_date: '2026-09-08 13:31'
labels:
  - tui
  - idea
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Filed from sbt 0.4.0 while at page=Tasks view=custom filter="" sort= selected=CKM-6 pane=None.

Impact: bottom command / status bar should show last action taken and time
Evidence: screen and action trail below, captured at filing time.

## Screen

```text
 [Tasks]   Pull Requests   tab switch page
┌ CambridgeKitchens  custom · cols:id,status,priority,title,ball · hide:done · group:project · Getting Financing · 2┐
│1 id 2 status    3 pri 4 title                                                                               5 ball│
│▸ First Internet Bank · In Progress · 0/1                                                                          │
│2    Waiting     M     First Internet Bank: await Charles Wheaton's underwriting read                              │
│▸ First National Bank · In Progress · 0/1                                                                          │
│6    In Progress M     First National Bank: send signed LOI and current structure                            me    │
│▸ GoSBA Loans · In Progress · 1/4                                                                                  │
│9    Waiting     H     GoSBA Loans: document request from Ishan (2026-09-02)                                       │
│9.5  Waiting     L     Finalize and upload SBA 1919 (MCC + CK) to GoSBA                                            │
│7    Waiting     M     GoSBA Loans: lender track - awaiting term sheet once 413 is uploaded                        │
│▸ Lender package · In Progress · 7/8                                                                               │
│11   Waiting     L     Hand-complete both 1919s: Q4 initials, SSN, signature/date, Addendum A on CK form     me    │
│▸ LendingClub · In Progress · 0/1                                                                                  │
│15   Waiting     M     LendingClub: await Brian Lawlor's read on the signed LOI                                    │
│▸ NewtekOne · In Progress · 1/2                                                                                    │
│5    In Progress M     NewtekOne: return signed prequal letter and complete portal forms                     me    │
│▸ Port 51 · In Progress · 0/8                                                                                      │
│1    To Do       M     Port 51: provide Hugh's requested lender package                                      me    │
│1.1  To Do       H     Provide signed SBA 413 for each guarantor                                             me    │
│1.2  To Do       H     Provide three years of personal tax returns                                           me    │
│1.3  To Do       H     Provide three years of target corporate tax returns                                   me    │
│1.4  To Do       H     Provide 2026 interim P&L and balance sheet current within 90 days                     me    │
│1.5  To Do       H     Provide resume or bio                                                                 me    │
│1.6  To Do       M     Update business plan for Port 51                                                      me    │
│1.7  To Do       M     Finalize financial projections for Port 51                                            me    │
│▸ Seller financial diligence · In Progress · 0/10                                                                  │
│24   To Do       H     Ask Nick to explain sales discounts and confirm projection treatment                  me    │
│22   Waiting     H     Provide signed backlog completion schedule as of 8/31                                       │
│16   Waiting     H     Request 7/31 customer A/R and A/P detail from Peachtree                               me    │
│18   Waiting     H     Substantiate payroll, benefits, and year-end payroll classification                         │
│18.1 To Do       H     Ask Nick for current employee payroll register and pay-rate summary                   me    │
│17   Waiting     M     Reconcile Cambridge Kitchens and Cambridge Appliances intercompany balances           me    │
│21   Waiting     M     Resolve financial-reporting and operating-document gaps                                     │
│19   Waiting     M     Substantiate vehicle allocation                                                             │
│23   Waiting     L     Substantiate FY2024-25 advertising reduction                                                │
│20   Waiting     L     Substantiate historical add-back claims held open                                           │
│▸ Truliant · In Progress · 0/1                                                                                     │
│4    In Progress M     Truliant: send signed LOI and current structure                                       me    │
│                                                                                                                   │
│                                                                                                                   │
│                                                                                                                   │
│                                                                                                                   │
│                                                                                                                   │
│                                                                                                                   │
│                                                                                                                   │
│                                                                                                                   │
└───────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
:idea bottom command / status bar should show last action taken and time▏
```

## Action trail

```text
session_start 0.4.0
action organize_picker
action group (0.2ms)
action group project
action command (0.0ms)
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Reporter confirms the behaviour in sbt matches what they were trying to do
<!-- AC:END -->
