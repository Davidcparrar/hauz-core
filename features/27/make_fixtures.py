#!/usr/bin/env python3
"""Generates the synthetic DIAN/UBL fixtures for feature #27 into crates/core/tests/fixtures/ubl/.
Deterministic (fixed timestamps, seeded bytes); nothing here comes from the private corpus.
Run from the repo root: python3 features/27/make_fixtures.py
"""
import base64, pathlib, random, zipfile, io

OUT = pathlib.Path("crates/core/tests/fixtures/ubl")
OUT.mkdir(parents=True, exist_ok=True)
STAMP = (2026, 10, 3, 12, 0, 0)

UBL_NS = 'xmlns="urn:oasis:names:specification:ubl:schema:xsd:Invoice-2" xmlns:cbc="urn:cbc" xmlns:cac="urn:cac"'


def attached(inner_root, inner_body):
    """Outer AttachedDocument (decoy IssueDate/SenderParty) wrapping `inner_root` in CDATA."""
    return (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<AttachedDocument xmlns:cbc="urn:cbc" xmlns:cac="urn:cac">\n'
        '  <cbc:ID>SYN-27</cbc:ID>\n'
        '  <cbc:IssueDate>2026-09-02</cbc:IssueDate>\n'
        '  <cac:SenderParty><cac:PartyTaxScheme><cbc:RegistrationName>Decoy Sender S.A.S.</cbc:RegistrationName></cac:PartyTaxScheme></cac:SenderParty>\n'
        '  <cac:Attachment><cac:ExternalReference><cbc:Description><![CDATA[\n'
        f'<{inner_root} {UBL_NS}>\n{inner_body}</{inner_root}>\n'
        ']]></cbc:Description></cac:ExternalReference></cac:Attachment>\n'
        '</AttachedDocument>\n'
    ).encode()


FULL_BODY = (
    '  <cbc:ID>ACME-0001</cbc:ID>\n'
    '  <cbc:IssueDate>2026-09-01</cbc:IssueDate>\n'
    '  <cbc:DueDate>2026-09-20</cbc:DueDate>\n'
    '  <cac:InvoicePeriod><cbc:StartDate>2026-08-01</cbc:StartDate><cbc:EndDate>2026-08-31</cbc:EndDate></cac:InvoicePeriod>\n'
    '  <cac:AccountingSupplierParty><cac:Party>\n'
    '    <cac:PartyName><cbc:Name>Acme Luz</cbc:Name></cac:PartyName>\n'
    '    <cac:PartyTaxScheme><cbc:RegistrationName>Acme &amp; Luz S.A.S. E.S.P.</cbc:RegistrationName></cac:PartyTaxScheme>\n'
    '    <cac:PartyLegalEntity><cbc:RegistrationName>Acme Luz S.A.S. E.S.P.</cbc:RegistrationName></cac:PartyLegalEntity>\n'
    '  </cac:Party></cac:AccountingSupplierParty>\n'
    '  <cac:AccountingCustomerParty><cac:Party>\n'
    '    <cac:PartyTaxScheme><cbc:RegistrationName>Cliente Ejemplo</cbc:RegistrationName></cac:PartyTaxScheme>\n'
    '  </cac:Party></cac:AccountingCustomerParty>\n'
    '  <cac:PaymentMeans><cbc:ID>1</cbc:ID><cbc:PaymentDueDate>2026-09-25</cbc:PaymentDueDate></cac:PaymentMeans>\n'
    '  <cac:LegalMonetaryTotal>\n'
    '    <cbc:LineExtensionAmount currencyID="COP">154916.00</cbc:LineExtensionAmount>\n'
    '    <cbc:TaxInclusiveAmount currencyID="COP">184350.00</cbc:TaxInclusiveAmount>\n'
    '    <cbc:PayableAmount currencyID="COP">184350.00</cbc:PayableAmount>\n'
    '  </cac:LegalMonetaryTotal>\n'
    '  <cac:InvoiceLine><cbc:ID>1</cbc:ID><cbc:LineExtensionAmount currencyID="COP">154916.00</cbc:LineExtensionAmount></cac:InvoiceLine>\n'
)

NO_PERIOD_BODY = (
    '  <cbc:ID>GAS-0002</cbc:ID>\n'
    '  <cbc:IssueDate>2026-09-10</cbc:IssueDate>\n'
    '  <cac:AccountingSupplierParty><cac:Party>\n'
    '    <cac:PartyLegalEntity><cbc:RegistrationName>Gas Natural Ejemplo S.A.</cbc:RegistrationName></cac:PartyLegalEntity>\n'
    '  </cac:Party></cac:AccountingSupplierParty>\n'
    '  <cac:PaymentMeans><cbc:ID>1</cbc:ID><cbc:PaymentDueDate>2026-09-30</cbc:PaymentDueDate></cac:PaymentMeans>\n'
    '  <cac:LegalMonetaryTotal><cbc:PayableAmount currencyID="USD">99.5</cbc:PayableAmount></cac:LegalMonetaryTotal>\n'
)

CREDIT_NOTE_BODY = (
    '  <cbc:ID>NC-0003</cbc:ID>\n'
    '  <cbc:IssueDate>2026-09-11</cbc:IssueDate>\n'
    '  <cac:AccountingSupplierParty><cac:Party><cac:PartyName><cbc:Name>Nota Credito S.A.</cbc:Name></cac:PartyName></cac:Party></cac:AccountingSupplierParty>\n'
    '  <cac:LegalMonetaryTotal><cbc:PayableAmount currencyID="COP">1.00</cbc:PayableAmount></cac:LegalMonetaryTotal>\n'
)

PDF = b"%PDF-1.4\n% synthetic, incompressible payload follows\n" + random.Random(27).randbytes(2048)


def make_zip(path, xml):
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as z:
        for name, data in (("ad09012345678900001.xml", xml), ("fv09012345678900001.pdf", PDF)):
            zi = zipfile.ZipInfo(name, date_time=STAMP)
            zi.compress_type = zipfile.ZIP_DEFLATED
            zi._compresslevel = 6
            z.writestr(zi, data)
    data = buf.getvalue()
    (OUT / path).write_bytes(data)
    return data


def make_eml(path, zip_bytes):
    b64 = base64.encodebytes(zip_bytes).decode().replace("\n", "\r\n")
    eml = (
        "From: facturacion@acme-luz.example\r\n"
        "To: pagos@hauz.example\r\n"
        "Subject: Factura electronica\r\n"
        "Date: Tue, 1 Sep 2026 09:00:00 -0500\r\n"
        "MIME-Version: 1.0\r\n"
        'Content-Type: multipart/mixed; boundary="b27"\r\n'
        "\r\n"
        "--b27\r\n"
        "Content-Type: text/plain; charset=us-ascii\r\n"
        "\r\n"
        "Adjunto factura electronica.\r\n"
        "--b27\r\n"
        'Content-Type: application/zip; name="z0001.zip"\r\n'
        "Content-Transfer-Encoding: base64\r\n"
        'Content-Disposition: attachment; filename="z0001.zip"\r\n'
        "\r\n" + b64 + "--b27--\r\n"
    )
    (OUT / path).write_bytes(eml.encode())


full_xml = attached("Invoice", FULL_BODY)
(OUT / "dian_full.xml").write_bytes(full_xml)
full_zip = make_zip("dian_full.zip", full_xml)
no_period_zip = make_zip("dian_no_period.zip", attached("Invoice", NO_PERIOD_BODY))
make_zip("not_invoice.zip", attached("CreditNote", CREDIT_NOTE_BODY))
make_eml("dian_full.eml", full_zip)
make_eml("dian_no_period.eml", no_period_zip)
make_eml("dian_corrupt.eml", full_zip[:-10])
