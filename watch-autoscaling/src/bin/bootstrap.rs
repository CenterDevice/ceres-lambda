use lambda_runtime::lambda;
use watch_autoscaling::lambda_handler;

fn main() {
    lambda!(lambda_handler)
}
