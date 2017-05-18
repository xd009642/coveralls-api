# Coveralls-api for rust

![Build Status](https://travis-ci.org/xd009642/coveralls-api.svg?branch=master) [![Latest Version](https://img.shields.io/crates/v/coveralls-api.svg)](https://crates.io/crates/coveralls-api)  [![License:MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

This project is intended to allow rust code to make requests to the coveralls.io API. Created to aid the development of cargo-tarpaulin. It allows you to build up a coveralls report for each source file using the Source struct, then package them up in the Report struct with tokens used to identify the repository and then send them to https://coveralls.io or a custom endpoint.

For an example of creating a report and sending it to coveralls.io, check out fill_in_example.rs in the tests directory. This test builds up a report and sends it to coveralls.

Currently, coveralls-api is feature complete with the free version of coveralls and some paid features. As such there is no roadmap or plans for future developments.

If you use coveralls and spot any issues please let me know or submit a PR yourself. Any contributions are welcome.
