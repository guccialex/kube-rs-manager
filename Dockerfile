FROM rustlang/rust:nightly


WORKDIR /home
COPY . .


#RUN rustup update nightly
#RUN rustup default nightly
RUN cargo update


RUN cargo build --release
CMD cargo run --release