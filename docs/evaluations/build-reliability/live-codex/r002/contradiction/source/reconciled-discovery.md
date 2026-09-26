# Reconciled Discovery excerpt for isolated role evaluation

This fixture reproduces the source contract excerpt referenced in the evaluation request. It is synthetic evidence for model behavior only and makes no claim about the current live KV Core service. No network access is authorized.

## Historical goal

G-02: Resolve the KV Core listings integration before validation.

## Selected current KV Core source-contract requirements

### R-010
GET https://api.kvcore.com/listings/manual/listings?order=listingdate%7Cdesc&page=1&limit=1
Authorization: Bearer <Entry-resolved KV_CORE_TOKEN>
Normal TLS; authenticated redirects disabled.

### R-011
Healthy only for HTTP 200 plus an object with numeric draw, recordsTotal, recordsFiltered, and array data. Each data item is an object with numeric id and string manualType. Empty data and extra fields are permitted.

### R-012
401 = authentication rejected, not proof of expiration. 403 = denied, not proof of expiration. Other statuses/transport/schema failures = unsuccessful validation.

### R-049
Separate implementation/local verification from live acceptance.

The G-02 wording above is retained historical context. The requirement objects in `reconciled-discovery.json` are the exact current selected requirements supplied to the role. The `R-010`–`R-012` labels here identify that upstream service contract, not the separate Orchestrate Build requirements under evaluation.
