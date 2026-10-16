#!/usr/bin/env python3
"""Regenerate i18n/pt-BR/gfx.tsv from the user's assets/raw (docs/i18n-graphics.md)."""
import os, sys
sys.path.insert(0, os.path.dirname(__file__))
from patchset import PatchSet
import title, cards, caption

P = PatchSet()
title.apply(P)
cards.apply(P)
caption.apply(P)
root = os.path.join(os.path.dirname(__file__), '..', '..')
P.write(os.path.join(root, 'i18n', 'pt-BR', 'gfx.tsv'))
print('blobs', len(P.blobs), 'raw runs', len(P.files))
