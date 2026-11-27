use evalexpr::{build_operator_tree, ContextWithMutableVariables, HashMapContext, Value};
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ExprError {
    #[error("Evaluation error: {0}")]
    Eval(String),
    #[error("Inversion failed for equation '{0}' at value {1}")]
    InversionFailed(String, f64),
}

impl From<evalexpr::EvalexprError> for ExprError {
    fn from(err: evalexpr::EvalexprError) -> Self {
        ExprError::Eval(err.to_string())
    }
}

fn sanitize_equation(equation: &str) -> String {
    equation.replace('[', "(").replace(']', ")")
}

pub fn evaluate_scaling(equation: &str, raw_value: f64) -> Result<f64, ExprError> {
    let eq = equation.trim();
    if eq.is_empty() || eq.eq_ignore_ascii_case("x") {
        return Ok(raw_value);
    }

    let sanitized = sanitize_equation(eq);
    let mut context = HashMapContext::new();
    context.set_value("X".to_string(), Value::from_float(raw_value))?;
    context.set_value("x".to_string(), Value::from_float(raw_value))?;

    let tree = build_operator_tree(&sanitized)?;
    let res = tree.eval_with_context(&context)?;

    match res {
        Value::Float(f) => Ok(f),
        Value::Int(i) => Ok(i as f64),
        Value::Boolean(b) => Ok(if b { 1.0 } else { 0.0 }),
        _ => Ok(raw_value),
    }
}

pub fn eval_scaling(equation: &str, raw_value: f64) -> Result<f64, ExprError> {
    evaluate_scaling(equation, raw_value)
}

pub fn invert_scaling(equation: &str, physical_value: f64) -> Result<f64, ExprError> {
    let eq = equation.trim();
    if eq.is_empty() || eq.eq_ignore_ascii_case("x") {
        return Ok(physical_value);
    }

    // 1. Try linear / affine inversion: f(x) = a * x + b
    let y0 = evaluate_scaling(eq, 0.0)?;
    let y1 = evaluate_scaling(eq, 1.0)?;
    let b = y0;
    let a = y1 - y0;

    if a.is_finite() && a.abs() > 1e-12 {
        let candidate = (physical_value - b) / a;
        if candidate.is_finite() {
            if let Ok(y_check) = evaluate_scaling(eq, candidate) {
                let diff = (y_check - physical_value).abs();
                let tol = 1e-4 * physical_value.abs().max(1.0);
                if diff <= tol {
                    return Ok(candidate);
                }
            }
        }
    }

    // 2. Newton-Raphson numerical root finding for g(x) = f(x) - physical_value = 0
    let mut x = if physical_value.abs() > 1e-6 {
        physical_value
    } else {
        1.0
    };

    for _ in 0..60 {
        let y = match evaluate_scaling(eq, x) {
            Ok(v) => v,
            Err(_) => break,
        };
        let err = y - physical_value;
        if err.abs() < 1e-7 {
            return Ok(x);
        }

        let h = 1e-5 * x.abs().max(1.0);
        let y_plus = match evaluate_scaling(eq, x + h) {
            Ok(v) => v,
            Err(_) => break,
        };
        let dy = (y_plus - y) / h;
        if dy.abs() < 1e-12 || !dy.is_finite() {
            break;
        }

        let step = err / dy;
        x -= step;
        if !x.is_finite() {
            break;
        }
        if step.abs() < 1e-9 {
            return Ok(x);
        }
    }

    // Check if the best candidate is acceptable
    if let Ok(final_y) = evaluate_scaling(eq, x) {
        if (final_y - physical_value).abs() < 1e-3 * physical_value.abs().max(1.0) {
            return Ok(x);
        }
    }

    Err(ExprError::InversionFailed(
        equation.to_string(),
        physical_value,
    ))
}
