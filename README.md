# RuClei - Rust Nuclei Clone

A fast, customizable vulnerability scanner written in Rust, inspired by [ProjectDiscovery's Nuclei](https://github.com/projectdiscovery/nuclei). RuClei parses and executes Nuclei-compatible YAML templates to identify vulnerabilities in web applications, APIs, and network services.

## Features

✅ **Nuclei Template Compatibility** - Parse and execute YAML templates in Nuclei format  
✅ **Request Clustering** - Avoid duplicate HTTP requests through intelligent caching  
✅ **Rate Limiting** - Configurable request rate limiting to avoid overwhelming targets  
✅ **Multiple Output Formats** - Support for text, JSON, and YAML output formats  
✅ **Flexible Matching** - Support for status, size, word, regex, binary, and DSL matchers  
✅ **Data Extraction** - Extract data using regex, key-value, JSON path, and DSL extractors  
✅ **Template Filtering** - Filter templates by severity, tags, or template IDs  
✅ **Statistics** - Detailed scan statistics including cache hit rates and performance metrics  
✅ **Single-threaded** - Designed for controlled, sequential scanning  

## Installation

### From Source

```bash
git clone https://github.com/your-repo/ruclei
cd ruclei
cargo build --release
```

The binary will be available at `./target/release/ruclei`.

## Usage

### Basic Usage

```bash
# Scan a single target
ruclei -u https://example.com -t templates/

# Scan multiple targets from a file
ruclei -l targets.txt -t templates/

# Scan with specific templates
ruclei -u https://example.com -t templates/http/ -t templates/ssl/

# Output results to JSON file
ruclei -u https://example.com -t templates/ -f json -o results.json
```

### Advanced Usage

```bash
# Rate limiting and delays
ruclei -u https://example.com -t templates/ --rate-limit 5.0 --delay 200

# Filter by severity and tags
ruclei -u https://example.com -t templates/ --severity high,critical --tags cve,rce

# Verbose output with statistics
ruclei -u https://example.com -t templates/ -v --stats

# Custom headers and proxy
ruclei -u https://example.com -t templates/ -H "Authorization: Bearer token" --proxy http://proxy:8080
```

### Command Line Options

```
Options:
  -u, --target <URL>            Target URL to scan
  -l, --list <FILE>             File containing list of target URLs
  -t, --templates <PATH>        Template file or directory
  -o, --output <FILE>           Output file to write results
  -f, --format <FORMAT>         Output format [text, json, yaml]
  -r, --rate-limit <RPS>        Rate limit in requests per second [default: 10.0]
  -d, --delay <MS>              Delay between requests in milliseconds
      --timeout <SECONDS>       HTTP timeout in seconds [default: 30]
      --max-redirects <NUM>     Maximum redirects to follow [default: 3]
      --user-agent <STRING>     User agent string [default: ruclei/1.0]
  -H, --header <HEADER>         Custom header (format: 'Name: Value')
      --proxy <URL>             Proxy URL
  -v, --verbose                 Verbose output
  -s, --silent                  Silent mode (only show matches)
      --max-cache-size <NUM>    Maximum number of cached requests [default: 1000]
      --severity <LEVEL>        Filter by severity (info,low,medium,high,critical)
      --tags <TAG>              Filter by tags
      --include-templates <ID>  Include specific template IDs
      --exclude-templates <ID>  Exclude specific template IDs
      --max-retries <NUM>       Maximum retries for failed requests [default: 3]
      --stats                   Show scan statistics
```

## Template Format

RuClei supports Nuclei-compatible YAML templates. Here's a basic example:

```yaml
id: basic-http-test
info:
  name: Basic HTTP Test
  author: [ruclei]
  severity: info
  description: A simple HTTP test template
  tags: [test, http]

requests:
  - method: GET
    path:
      - "/"
      - "/robots.txt"
    
    matchers:
      - type: status
        status:
          - 200
        name: "success"
    
    extractors:
      - type: regex
        name: "title"
        regex:
          - '<title>([^<]+)</title>'
        part: body
        group: 1
```

### Supported Matchers

- **status**: Match HTTP status codes
- **size**: Match response content length
- **word**: Match words in response
- **regex**: Match regex patterns
- **binary**: Match binary patterns (hex)
- **dsl**: Match using DSL expressions

### Supported Extractors

- **regex**: Extract using regex patterns
- **kval**: Extract key-value pairs from headers/body
- **json**: Extract from JSON responses using path expressions
- **dsl**: Extract using DSL expressions
- **xpath**: Extract using XPath (basic implementation)

## Architecture

RuClei is built with a modular architecture:

- **Template Parser**: Parses YAML templates into Rust structures
- **HTTP Client**: Handles HTTP requests with retry logic and timeout
- **Request Cluster**: Caches requests to avoid duplicates
- **Rate Limiter**: Controls request frequency
- **Matcher Engine**: Evaluates response matchers
- **Extractor Engine**: Extracts data from responses
- **CLI Interface**: Command-line argument parsing and configuration

## Request Clustering

RuClei implements intelligent request clustering to avoid sending duplicate HTTP requests. Requests are clustered based on:

- URL
- HTTP method
- Headers
- Request body

This significantly improves performance when multiple templates test the same endpoints.

## Rate Limiting

The rate limiter supports multiple modes:

- **Requests per second**: `--rate-limit 10.0`
- **Fixed delay**: `--delay 1000` (milliseconds)
- **Combined**: Both rate limiting and minimum delay

## Performance

Example scan statistics:

```
=== Scan Statistics ===
Templates loaded: 150
Templates executed: 150
Total requests: 450
Successful requests: 445
Failed requests: 5
Success rate: 98.89%
Matches found: 23
Cache hits: 125
Cache misses: 325
Cache hit rate: 27.78%
Scan duration: 45.23s
Average RPS: 9.95
```

## Differences from Nuclei

While RuClei aims for compatibility with Nuclei templates, there are some differences:

1. **Single-threaded**: RuClei is designed for sequential scanning
2. **Simplified DSL**: Basic DSL support (not full Nuclei DSL compatibility)
3. **Limited protocols**: Currently focuses on HTTP/HTTPS (no DNS, network protocols yet)
4. **Basic XPath**: Simplified XPath implementation

## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- [ProjectDiscovery](https://github.com/projectdiscovery) for creating Nuclei and the template format
- The Rust community for excellent HTTP and parsing libraries

