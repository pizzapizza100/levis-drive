# Multi-stage docker file:

# Build Stage

# Get rust at my version, potentially this can be rust::latest.
FROM rust:1.80.1 AS builder 
# Create the working directory ( if it doesn't exists already ) and moves into it ( mkdir -p /app && cd /app )
WORKDIR /app

# Copy only the dependencies first
COPY Cargo.toml Cargo.lock /app/

# Copies everything from the current folder and puts it into the current working directory inside the container.
COPY ./src /app/src
# Build app at release mode
RUN cargo build --release

# Run Stage
# Get debian's version of rust ( smaller than normal, just for running it ). 
FROM debian:bookworm-slim
# Create the working directory ( if it doesn't exists already ) and moves into it ( mkdir -p /app && cd /app )
WORKDIR /app
# Copies the compiled files from the builder stage.
COPY --from=builder /app/target/release/LevisDrive .
# Run the app in the cmd
CMD ["./LevisDrive"]