# Coveralls-api for rust

This project is intended to allow rust code to make requests to the coveralls.io API. Created to aid the development of cargo-tarpaulin. Currently it lets you build up a coveralls report with no mechanisms to send it.

If you wish to use this crate, use serde_json to generate a json string and then a HTTP library of your choice to POST the data to coveralls.io.
