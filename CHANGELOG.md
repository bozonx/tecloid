# Changelog

All notable changes to tecloid are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
New entries go under `[Unreleased]`; `cargo release` turns it into a dated version section.

## [Unreleased]

- Mapper: a layer key whose tap runs a shortcut (CapsLock: tap `Ctrl+KeyV`, hold a layer) stays the layer when it is released just before a key the layer maps, if both were held together for at least 50 ms. A quick CapsLock+J or CapsLock+Period no longer pastes and types the key itself ("ю" on a Russian layout). Typing taps (Space, Tab, `text:`) and short typing rolls are unchanged.
