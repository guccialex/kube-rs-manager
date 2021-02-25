FROM rust

WORKDIR /home
COPY . .

RUN cargo install --path .
RUN rustup update nightly
RUN rustup default nightly

#RUN cargo update
RUN cargo build --release


CMD cargo run --release