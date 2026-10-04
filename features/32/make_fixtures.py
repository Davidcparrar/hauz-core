#!/usr/bin/env python3
"""Generates the synthetic zip fixtures for feature #32 into crates/core/tests/fixtures/zip/.
Deterministic (fixed timestamps, seeded bytes); nothing here comes from the private corpus.
Run from the repo root: python3 features/32/make_fixtures.py
"""
import pathlib, random, zipfile

OUT = pathlib.Path("crates/core/tests/fixtures/zip")
OUT.mkdir(parents=True, exist_ok=True)
STAMP = (2026, 10, 3, 12, 0, 0)


def info(name):
    return zipfile.ZipInfo(name, date_time=STAMP)


def write(path, entries, method, level=None):
    with zipfile.ZipFile(OUT / path, "w") as z:
        for name, data in entries:
            zi = info(name)
            zi.compress_type = method
            if level is not None:
                zi._compresslevel = level
            z.writestr(zi, data)


XML = ('<?xml version="1.0" encoding="utf-8"?>\n<AttachedDocument xmlns:cbc="urn:cbc" xmlns:cac="urn:cac">\n'
       '  <cbc:ID>SYN-0001</cbc:ID>\n  <cac:Attachment><cac:ExternalReference><cbc:Description><![CDATA[\n'
       '<Invoice xmlns="urn:oasis:names:specification:ubl:schema:xsd:Invoice-2" xmlns:cbc="urn:cbc" xmlns:cac="urn:cac">\n'
       + "".join(f'  <cac:InvoiceLine><cbc:ID>{i}</cbc:ID><cbc:LineExtensionAmount currencyID="COP">{i * 1000}.00'
                 f'</cbc:LineExtensionAmount><cac:Item><cbc:Description>Synthetic line item {i}</cbc:Description></cac:Item></cac:InvoiceLine>\n'
                 for i in range(1, 41))
       + '  <cac:LegalMonetaryTotal><cbc:PayableAmount currencyID="COP">820000.00</cbc:PayableAmount></cac:LegalMonetaryTotal>\n'
       '</Invoice>\n]]></cbc:Description></cac:ExternalReference></cac:Attachment>\n</AttachedDocument>\n').encode()
PDF = b"%PDF-1.4\n% synthetic, incompressible payload follows\n" + random.Random(32).randbytes(4096)

(OUT / "ad000000001.xml").write_bytes(XML)
(OUT / "ad000000001.pdf").write_bytes(PDF)
write("stored.zip", [("folder/", b""), ("folder/hello.txt", b"hello, zip\n"), ("empty.txt", b"")], zipfile.ZIP_STORED)
write("small.zip", [("a.txt", b"abcabcabcabc\n")], zipfile.ZIP_DEFLATED, 9)
write("invoice.zip", [("ad000000001.xml", XML), ("ad000000001.pdf", PDF)], zipfile.ZIP_DEFLATED, 6)
write("bzip2.zip", [("a.txt", b"abcabcabcabc\n")], zipfile.ZIP_BZIP2)
write("empty.zip", [], zipfile.ZIP_STORED)
