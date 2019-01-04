use aws_watchtower::lambda_handler;
use lambda_runtime::lambda;

fn main() {
    lambda!(lambda_handler)
}
