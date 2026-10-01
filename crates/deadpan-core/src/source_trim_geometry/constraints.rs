//! A fixed set of affine inequalities in [In, Out, Slip, Roll]. Each active
//! coefficient is -1, 0 or 1, so adjustment needs no search or frame expansion.

use super::*;

struct Constraint {
    constant: ExactRatio,
    coefficients: [i8; 4],
    inclusive: bool,
    owner: SourceTrimGeometryOwner,
    reason: SourceTrimGeometryConstraint,
}

impl Constraint {
    fn value(&self, intent: SourceTrimIntent) -> Result<ExactRatio, TimeError> {
        let integer = self.coefficients.iter().zip(intent.values()).try_fold(
            0i128,
            |sum, (coefficient, value)| {
                sum.checked_add(i128::from(*coefficient) * i128::from(value))
                    .ok_or(TimeError::Overflow)
            },
        )?;
        self.constant.checked_add(ExactRatio::new(integer, 1)?)
    }
}

pub(super) struct Constraints(Vec<Constraint>);

impl Constraints {
    fn push(
        &mut self,
        constant: ExactRatio,
        coefficients: [i8; 4],
        inclusive: bool,
        owner: SourceTrimGeometryOwner,
        reason: SourceTrimGeometryConstraint,
    ) {
        self.0.push(Constraint {
            constant,
            coefficients,
            inclusive,
            owner,
            reason,
        });
    }

    pub(super) fn validate(&self, intent: SourceTrimIntent) -> Result<(), DocumentError> {
        for constraint in &self.0 {
            let order = constraint.value(intent)?.compare(ExactRatio::ZERO);
            if order.is_lt() || (order.is_eq() && !constraint.inclusive) {
                return Err(match constraint.reason {
                    SourceTrimGeometryConstraint::PhysicalDuration
                    | SourceTrimGeometryConstraint::ProjectDuration
                    | SourceTrimGeometryConstraint::IntegerValue => TimeError::Overflow.into(),
                    _ => invalid(&format!(
                        "combined Source trim violates {:?} {:?}",
                        constraint.owner, constraint.reason
                    )),
                });
            }
        }
        Ok(())
    }

    pub(super) fn limits(
        &self,
        intent: SourceTrimIntent,
        control: SourceTrimControl,
    ) -> Result<(SourceTrimGeometryLimit, SourceTrimGeometryLimit, i64, i64), TimeError> {
        let mut minimum = SourceTrimGeometryLimit {
            value: ExactRatio::integer(i64::MIN),
            inclusive: true,
            owner: SourceTrimGeometryOwner::Intent,
            constraint: SourceTrimGeometryConstraint::IntegerValue,
        };
        let mut maximum = SourceTrimGeometryLimit {
            value: ExactRatio::integer(i64::MAX),
            ..minimum
        };
        let others = intent.with_value(control, 0);
        for constraint in &self.0 {
            // Storage/arithmetic limits fail explicitly. They are not media
            // handles and must not masquerade as a clamped scalar edit.
            if matches!(
                constraint.reason,
                SourceTrimGeometryConstraint::PhysicalDuration
                    | SourceTrimGeometryConstraint::ProjectDuration
            ) {
                continue;
            }
            let coefficient = constraint.coefficients[control.index()];
            if coefficient == 0 {
                continue;
            }
            let value = constraint.value(others)?;
            let lower = coefficient > 0;
            let candidate = SourceTrimGeometryLimit {
                value: if lower {
                    ExactRatio::ZERO.checked_sub(value)?
                } else {
                    value
                },
                inclusive: constraint.inclusive,
                owner: constraint.owner,
                constraint: constraint.reason,
            };
            let current = if lower { &mut minimum } else { &mut maximum };
            let order = candidate.value.compare(current.value);
            if (lower && order.is_gt())
                || (!lower && order.is_lt())
                || (order.is_eq() && current.inclusive && !candidate.inclusive)
            {
                *current = candidate;
            }
        }
        let minimum_value = integer_bound(minimum, true)?;
        let maximum_value = integer_bound(maximum, false)?;
        if minimum_value > maximum_value {
            return Err(TimeError::InvalidRatio);
        }
        Ok((minimum, maximum, minimum_value, maximum_value))
    }
}

fn integer_bound(limit: SourceTrimGeometryLimit, lower: bool) -> Result<i64, TimeError> {
    let value = match (lower, limit.inclusive) {
        (true, true) => limit.value.ceil()?,
        (true, false) => limit
            .value
            .floor()
            .checked_add(1)
            .ok_or(TimeError::Overflow)?,
        (false, true) => limit.value.floor(),
        (false, false) => limit
            .value
            .ceil()?
            .checked_sub(1)
            .ok_or(TimeError::Overflow)?,
    };
    i64::try_from(value).map_err(|_| TimeError::Overflow)
}

pub(super) fn context_constraints(
    context: &Context<'_>,
    policy: SourceTrimPolicy,
) -> Result<Constraints, DocumentError> {
    use SourceTrimGeometryConstraint as Reason;
    use SourceTrimGeometryOwner as Owner;
    let mut constraints = Constraints(Vec::with_capacity(20));
    let a = &context.target;
    constraints.push(
        a.effective.start().checked_sub(a.video_support.start)?,
        [1, 0, 1, 0],
        true,
        Owner::Target,
        Reason::PictureStart,
    );
    constraints.push(
        a.video_support.end.checked_sub(a.effective.end())?,
        [0, -1, -1, -1],
        true,
        Owner::Target,
        Reason::PictureEnd,
    );
    constraints.push(
        a.effective.end().checked_sub(a.effective.start())?,
        [-1, 1, 0, 1],
        false,
        Owner::Target,
        Reason::MinimumSelectedDuration,
    );
    constraints.push(
        ExactRatio::integer(a.allocation.duration().frames() - 1),
        [-1, 1, 0, 1],
        true,
        Owner::Target,
        Reason::MinimumOutputDuration,
    );
    // Grow-only owner storage: old_duration+prefix and new_end+prefix fit i64.
    // Both forms of max(0,-start) are admitted without an intermediate owner.
    constraints.push(
        integer(
            i128::from(i64::MAX) - i128::from(a.source.duration.frames())
                + i128::from(a.allocation.start().0),
        )?,
        [1, 0, 0, 0],
        true,
        Owner::Target,
        Reason::PhysicalDuration,
    );
    constraints.push(
        integer(i128::from(i64::MAX) - i128::from(a.allocation.end().0))?,
        [0, -1, 0, -1],
        true,
        Owner::Target,
        Reason::PhysicalDuration,
    );
    constraints.push(
        integer(i128::from(i64::MAX) - i128::from(a.allocation.duration().frames()))?,
        [1, -1, 0, -1],
        true,
        Owner::Target,
        Reason::PhysicalDuration,
    );
    if let Some(b) = &context.right {
        constraints.push(
            b.effective.start().checked_sub(b.video_support.start)?,
            [0, 0, 0, 1],
            true,
            Owner::Right,
            Reason::PictureStart,
        );
        constraints.push(
            b.effective.end().checked_sub(b.effective.start())?,
            [0, 0, 0, -1],
            false,
            Owner::Right,
            Reason::MinimumSelectedDuration,
        );
        constraints.push(
            ExactRatio::integer(b.allocation.duration().frames() - 1),
            [0, 0, 0, -1],
            true,
            Owner::Right,
            Reason::MinimumOutputDuration,
        );
        constraints.push(
            integer(
                i128::from(i64::MAX) - i128::from(b.source.duration.frames())
                    + i128::from(b.allocation.start().0),
            )?,
            [0, 0, 0, 1],
            true,
            Owner::Right,
            Reason::PhysicalDuration,
        );
        constraints.push(
            integer(i128::from(i64::MAX) - i128::from(b.allocation.duration().frames()))?,
            [0, 0, 0, 1],
            true,
            Owner::Right,
            Reason::PhysicalDuration,
        );
    }
    match policy {
        SourceTrimPolicy::Ripple => {
            constraints.push(
                ExactRatio::integer(context.total.frames()),
                [-1, 1, 0, 0],
                true,
                Owner::Project,
                Reason::ProjectDuration,
            );
            constraints.push(
                ExactRatio::integer(i64::MAX - context.total.frames()),
                [1, -1, 0, 0],
                true,
                Owner::Project,
                Reason::ProjectDuration,
            );
        }
        SourceTrimPolicy::Overwrite => {
            constraints.push(
                ExactRatio::integer(a.output.start().0 - context.scope.start().0),
                [1, 0, 0, 0],
                true,
                Owner::Scope,
                Reason::ScopeStart,
            );
            constraints.push(
                ExactRatio::integer(context.scope.end().0 - a.output.end().0),
                [0, -1, 0, -1],
                true,
                Owner::Scope,
                Reason::ScopeEnd,
            );
        }
    }
    Ok(constraints)
}

fn integer(value: i128) -> Result<ExactRatio, TimeError> {
    ExactRatio::new(value, 1)
}
