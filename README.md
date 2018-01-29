# Music Catalog

A Rust program to manage a Postgres database and Elasticsearch instance of my music collection.

## Setup

 - `.env`
   - Add `DATABASE_URL` with a standard PostgresDB connection string according to [Section 31.1.1.](https://www.postgresql.org/docs/9.3/static/libpq-connect.html#LIBPQ-CONNSTRING).
   - Add `ELASTICSEARCH_URL` with the url to the Elasticsearch instance (like `http://example.com:9200`).
 - `config.yaml`
   - Copy `config.yaml.example` to `config.yaml`
   - Obtain an AcoustID API key from https://acoustid.org, and add it under `api_keys.acoustid`
   - Add the paths to your media files to the `paths` list

## Usage

This crate uses `log` and log verbosity can be controlled by `RUST_LOG`.

#### Scanning

`catalogcli scan`

Re-scans the paths under the `paths` list in `config.yaml`. Adds new media file entries to the database. *Does not remove entries that are no longer accessible.*

#### Pruning

`catalogcli prune`

Checks database for files that are no longer accessible via a simple path existance check.

#### Testing commands

There are other commands implemented for usage in testing single modules of this project. Read the help output from `catalogcli --help` to learn more.

## License

```
Copyright 2017 Matt Bilker

Permission is hereby granted, free of charge, to any person obtaining a copy of 
this software and associated documentation files (the "Software"), to deal in 
the Software without restriction, including without limitation the rights to 
use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies 
of the Software, and to permit persons to whom the Software is furnished to do 
so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all 
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR 
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, 
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE 
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER 
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, 
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE 
SOFTWARE.
```
