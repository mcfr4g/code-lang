use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Write};

use num_bigint::{BigInt, Sign};
use num_traits::{ToPrimitive, Zero};

use crate::error::{quoted, CodeError, Result};
use crate::number::Number;
use crate::syntax::{self, Compare, Condition, Expr, ExprKind, Name, Statement, StatementKind};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct CollectionId(usize);

#[derive(Clone, Debug)]
enum Value {
    Number(Number),
    Text(String),
    Truth(bool),
    Collection(CollectionId),
}

impl Value {
    fn collection(&self, line: usize) -> Result<CollectionId> {
        self.collection_id()
            .ok_or_else(|| CodeError::new(line, "Expected a collection"))
    }

    fn collection_id(&self) -> Option<CollectionId> {
        if let Self::Collection(id) = self {
            Some(*id)
        } else {
            None
        }
    }
}

#[derive(Debug)]
struct Item {
    value: Value,
    name: Option<String>,
}

#[derive(Debug, Default)]
struct Collection {
    items: Vec<Item>,
    names: HashMap<String, usize>,
    active_loops: usize,
}

impl Collection {
    fn check_resize(&self, line: usize) -> Result<()> {
        if self.active_loops > 0 {
            return Err(CodeError::new(
                line,
                "Cannot change a collection's size during for each",
            ));
        }
        Ok(())
    }

    fn index(&self, position: &BigInt, line: usize) -> Result<usize> {
        position
            .to_usize()
            .and_then(|index| index.checked_sub(1))
            .filter(|&index| index < self.items.len())
            .ok_or_else(|| {
                CodeError::new(
                    line,
                    format!(
                        "Position {position} is out of range (collection has {} items)",
                        self.items.len()
                    ),
                )
            })
    }

    fn named_index(&self, name: &str, line: usize) -> Result<usize> {
        self.names
            .get(name)
            .copied()
            .ok_or_else(|| CodeError::new(line, format!("No item named {}", quoted(name))))
    }

    fn append(&mut self, value: Value, name: Option<String>, line: usize) -> Result<()> {
        self.check_resize(line)?;
        if let Some(name) = &name {
            if self.names.contains_key(name) {
                return Err(CodeError::new(
                    line,
                    format!("Duplicate item name {}", quoted(name)),
                ));
            }
            self.names.insert(name.clone(), self.items.len());
        }
        self.items.push(Item { value, name });
        Ok(())
    }

    fn update(&mut self, name: &str, value: Value, line: usize) -> Result<()> {
        let index = self.named_index(name, line)?;
        self.items[index].value = value;
        Ok(())
    }

    fn remove(&mut self, position: &BigInt, line: usize) -> Result<()> {
        self.check_resize(line)?;
        let index = self.index(position, line)?;
        if let Some(name) = self.items.remove(index).name {
            self.names.remove(&name);
        }
        for (shifted_index, item) in self.items.iter().enumerate().skip(index) {
            if let Some(name) = &item.name {
                *self.names.get_mut(name).expect("named item is indexed") = shifted_index;
            }
        }
        Ok(())
    }
}

const MIN_COLLECTION_BUDGET: usize = 128;

struct Heap {
    slots: Vec<Option<Collection>>,
    free: Vec<CollectionId>,
    allocations_until_collection: usize,
}

impl Heap {
    fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            allocations_until_collection: MIN_COLLECTION_BUDGET,
        }
    }

    fn allocate(&mut self, value: Collection) -> CollectionId {
        self.allocations_until_collection = self.allocations_until_collection.saturating_sub(1);
        if let Some(id) = self.free.pop() {
            self.slots[id.0] = Some(value);
            id
        } else {
            let id = CollectionId(self.slots.len());
            self.slots.push(Some(value));
            id
        }
    }

    fn get(&self, id: CollectionId) -> &Collection {
        self.slots[id.0].as_ref().expect("collection is live")
    }

    fn get_mut(&mut self, id: CollectionId) -> &mut Collection {
        self.slots[id.0].as_mut().expect("collection is live")
    }

    fn collect(&mut self, roots: impl Iterator<Item = CollectionId>) {
        let mut marked = vec![false; self.slots.len()];
        let mut pending: Vec<_> = roots.collect();
        while let Some(id) = pending.pop() {
            if !marked[id.0] {
                marked[id.0] = true;
                pending.extend(
                    self.get(id)
                        .items
                        .iter()
                        .filter_map(|item| item.value.collection_id()),
                );
            }
        }
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if !marked[index] && slot.take().is_some() {
                self.free.push(CollectionId(index));
            }
        }
        let live = self.slots.len() - self.free.len();
        self.allocations_until_collection = live.max(MIN_COLLECTION_BUDGET);
    }
}

struct Iteration {
    collection: CollectionId,
    previous: Option<Value>,
}

pub fn run(source: &str, input: &mut impl BufRead, output: &mut impl Write) -> Result<()> {
    let program = syntax::parse(source)?;
    Interpreter {
        input,
        output,
        variables: HashMap::new(),
        heap: Heap::new(),
        iterations: Vec::new(),
    }
    .execute(&program)
}

struct Interpreter<'streams, R, W> {
    input: &'streams mut R,
    output: &'streams mut W,
    variables: HashMap<String, Value>,
    heap: Heap,
    iterations: Vec<Iteration>,
}

impl<R: BufRead, W: Write> Interpreter<'_, R, W> {
    fn execute(&mut self, statements: &[Statement]) -> Result<()> {
        for statement in statements {
            self.statement(statement)?;
            self.collect_if_needed();
        }
        Ok(())
    }

    // Call at statement boundaries, when no expression temporaries are live.
    fn collect_if_needed(&mut self) {
        if self.heap.allocations_until_collection == 0 {
            let variables = self.variables.values().filter_map(Value::collection_id);
            let active = self.iterations.iter().map(|iteration| iteration.collection);
            let saved = self
                .iterations
                .iter()
                .filter_map(|iteration| iteration.previous.as_ref())
                .filter_map(Value::collection_id);
            self.heap.collect(variables.chain(active).chain(saved));
        }
    }

    fn statement(&mut self, statement: &Statement) -> Result<()> {
        use StatementKind::*;

        let line = statement.line;
        match &statement.kind {
            Assign {
                variable,
                value,
                declare,
            } => {
                let name = &variable.text;
                let exists = self.variables.contains_key(name);
                if *declare && exists {
                    return Err(CodeError::new(
                        line,
                        format!("Variable {} already exists", quoted(name)),
                    ));
                }
                if !*declare && !exists {
                    return Err(CodeError::new(
                        line,
                        format!("Unknown variable {}", quoted(name)),
                    ));
                }
                let value = self.evaluate(value)?;
                self.variables.insert(name.clone(), value);
            }
            Adjust {
                variable,
                amount,
                subtract,
            } => {
                let current = self.read_variable(&variable.text, variable.line)?;
                let current = Self::as_number(current, variable.line)?;
                let change = self.number(amount)?;
                let result = current
                    .adjust(change, *subtract)
                    .ok_or_else(|| CodeError::new(line, "Number is too large"))?;
                self.variables
                    .insert(variable.text.clone(), Value::Number(result));
            }
            Print {
                value,
                numeric,
                newline,
            } => {
                let result = if *numeric {
                    Value::Number(self.number(value)?)
                } else {
                    self.evaluate(value)?
                };
                let mut text = String::new();
                self.render(&result, &mut text, &mut HashSet::new(), line)?;
                self.print(&text, *newline, line)?;
            }
            Ask { prompt, variable } => self.ask(prompt, variable, line)?,
            Count(collection) => {
                let id = self.collection(collection)?;
                self.print(&self.heap.get(id).items.len().to_string(), true, line)?;
            }
            Repeat(count, body) => {
                let count_error =
                    || CodeError::new(line, "Loop count must be a non-negative whole number");
                let Value::Number(Number::Whole(mut times)) = self.evaluate(count)? else {
                    return Err(count_error());
                };
                if times.sign() == Sign::Minus {
                    return Err(count_error());
                }
                if !body.is_empty() {
                    while !times.is_zero() {
                        self.execute(body)?;
                        times -= 1_u8;
                    }
                }
            }
            While(condition, body) => {
                while self.condition(condition)? {
                    self.collect_if_needed();
                    self.execute(body)?;
                }
            }
            If {
                condition,
                body,
                alternative,
            } => {
                let branch = if self.condition(condition)? {
                    body
                } else {
                    alternative
                };
                self.execute(branch)?;
            }
            ForEach {
                variable,
                collection,
                body,
            } => {
                let id = self.collection(collection)?;
                self.for_each(&variable.text, id, body)?;
            }
            Put { value, target } => {
                let value = self.evaluate(value)?;
                if let ExprKind::Named(target) = &target.kind {
                    let (last, parents) = target.names.split_last().expect("at least one name");
                    let (parent, parent_line) = self.named_path(&target.collection, parents)?;
                    let id = parent.collection(parent_line)?;
                    self.heap.get_mut(id).update(&last.text, value, line)?;
                } else {
                    let id = self.collection(target)?;
                    self.heap.get_mut(id).append(value, None, line)?;
                }
            }
            Remove(target) => {
                let id = self.collection(&target.collection)?;
                self.heap.get_mut(id).remove(&target.index, line)?;
            }
        }
        Ok(())
    }

    fn for_each(&mut self, name: &str, id: CollectionId, body: &[Statement]) -> Result<()> {
        let previous = self.variables.remove(name);
        self.iterations.push(Iteration {
            collection: id,
            previous,
        });
        let collection = self.heap.get_mut(id);
        collection.active_loops += 1;
        let count = collection.items.len();
        let result = (0..count).try_for_each(|index| {
            let value = self.heap.get(id).items[index].value.clone();
            self.variables.insert(name.to_owned(), value);
            self.execute(body)
        });
        self.heap.get_mut(id).active_loops -= 1;
        let iteration = self.iterations.pop().expect("active iteration");
        if let Some(previous) = iteration.previous {
            self.variables.insert(name.to_owned(), previous);
        } else {
            self.variables.remove(name);
        }
        result
    }

    fn evaluate(&mut self, expr: &Expr) -> Result<Value> {
        match &expr.kind {
            ExprKind::Number(value) => Ok(Value::Number(value.clone())),
            ExprKind::Text(value) => Ok(Value::Text(value.clone())),
            ExprKind::Truth(value) => Ok(Value::Truth(*value)),
            ExprKind::Variable(name) => self.read_variable(name, expr.line),
            ExprKind::Group(value) => self.evaluate(value),
            ExprKind::Position(position) => {
                let id = self.collection(&position.collection)?;
                let collection = self.heap.get(id);
                let index = collection.index(&position.index, expr.line)?;
                Ok(collection.items[index].value.clone())
            }
            ExprKind::Named(named) => {
                let (value, _) = self.named_path(&named.collection, &named.names)?;
                Ok(value)
            }
            ExprKind::Collection(entries) => {
                let mut collection = Collection::default();
                for entry in entries {
                    let value = self.evaluate(&entry.value)?;
                    collection.append(value, entry.name.clone(), entry.value.line)?;
                }
                Ok(Value::Collection(self.heap.allocate(collection)))
            }
        }
    }

    fn named_path(&mut self, root: &Expr, names: &[Name]) -> Result<(Value, usize)> {
        let mut value = self.evaluate(root)?;
        let mut line = root.line;
        for name in names {
            let collection = self.heap.get(value.collection(line)?);
            let index = collection.named_index(&name.text, name.line)?;
            value = collection.items[index].value.clone();
            line = name.line;
        }
        Ok((value, line))
    }

    fn read_variable(&self, name: &str, line: usize) -> Result<Value> {
        self.variables
            .get(name)
            .cloned()
            .ok_or_else(|| CodeError::new(line, format!("Unknown variable {}", quoted(name))))
    }

    fn number(&mut self, expr: &Expr) -> Result<Number> {
        Self::as_number(self.evaluate(expr)?, expr.line)
    }

    fn as_number(value: Value, line: usize) -> Result<Number> {
        if let Value::Number(number) = value {
            Ok(number)
        } else {
            Err(CodeError::new(line, "Expected a number"))
        }
    }

    fn collection(&mut self, expr: &Expr) -> Result<CollectionId> {
        self.evaluate(expr)?.collection(expr.line)
    }

    fn condition(&mut self, condition: &Condition) -> Result<bool> {
        let left = &condition.value;
        let Some((operator, right)) = &condition.comparison else {
            return if let Value::Truth(value) = self.evaluate(left)? {
                Ok(value)
            } else {
                Err(CodeError::new(
                    left.line,
                    "Expected true or false as the condition",
                ))
            };
        };
        match operator {
            Compare::Less | Compare::Greater => {
                let ordering = self.number(left)?.compare(&self.number(right)?);
                Ok(match operator {
                    Compare::Less => ordering == Ordering::Less,
                    _ => ordering == Ordering::Greater,
                })
            }
            Compare::Equal | Compare::NotEqual => {
                let equal = match (self.evaluate(left)?, self.evaluate(right)?) {
                    (Value::Number(left), Value::Number(right)) => {
                        left.compare(&right) == Ordering::Equal
                    }
                    (Value::Text(left), Value::Text(right)) => left == right,
                    (Value::Truth(left), Value::Truth(right)) => left == right,
                    (Value::Collection(_), _) | (_, Value::Collection(_)) => {
                        return Err(CodeError::new(
                            left.line,
                            "Equality requires numbers, text, or truth values",
                        ));
                    }
                    _ => false,
                };
                Ok(if matches!(operator, Compare::Equal) {
                    equal
                } else {
                    !equal
                })
            }
        }
    }

    fn ask(&mut self, prompt: &Expr, variable: &Name, line: usize) -> Result<()> {
        let name = &variable.text;
        if self.variables.contains_key(name) {
            return Err(CodeError::new(
                line,
                format!("Variable {} already exists", quoted(name)),
            ));
        }
        let Value::Text(message) = self.evaluate(prompt)? else {
            return Err(CodeError::new(prompt.line, "Expected text for the prompt"));
        };
        self.print(&message, true, line)?;
        self.output
            .flush()
            .map_err(|error| CodeError::new(line, format!("Output error: {error}")))?;
        let mut response = String::new();
        let bytes_read = self
            .input
            .read_line(&mut response)
            .map_err(|error| CodeError::new(line, format!("Input error: {error}")))?;
        if bytes_read == 0 {
            return Err(CodeError::no_input(line));
        }
        if response.ends_with('\n') {
            response.pop();
            if response.ends_with('\r') {
                response.pop();
            }
        }
        self.variables.insert(name.clone(), Value::Text(response));
        Ok(())
    }

    fn print(&mut self, text: &str, newline: bool, line: usize) -> Result<()> {
        let end = if newline { "\n" } else { "" };
        write!(self.output, "{text}{end}")
            .map_err(|error| CodeError::new(line, format!("Output error: {error}")))
    }

    fn render(
        &self,
        value: &Value,
        output: &mut String,
        ancestors: &mut HashSet<CollectionId>,
        line: usize,
    ) -> Result<()> {
        use std::fmt::Write as _;

        match value {
            Value::Number(value) => write!(output, "{value}").expect("writing to a string"),
            Value::Text(value) if ancestors.is_empty() => output.push_str(value),
            Value::Text(value) => {
                output.push_str(&serde_json::to_string(value).expect("JSON text"))
            }
            Value::Truth(value) => output.push_str(if *value { "true" } else { "false" }),
            Value::Collection(id) => {
                if ancestors.contains(id) {
                    output.push_str("<cycle>");
                    return Ok(());
                }
                if ancestors.len() >= syntax::MAX_NESTING {
                    return Err(CodeError::new(line, "Program is nested too deeply"));
                }
                ancestors.insert(*id);
                output.push('[');
                for (index, item) in self.heap.get(*id).items.iter().enumerate() {
                    if index > 0 {
                        output.push_str(", ");
                    }
                    self.render(&item.value, output, ancestors, line)?;
                    if let Some(name) = &item.name {
                        write!(output, " named {name}").expect("writing to a string");
                    }
                }
                output.push(']');
                ancestors.remove(id);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run;

    #[test]
    fn unreachable_self_reference_can_be_reclaimed() {
        let mut heap = Heap::new();
        let id = heap.allocate(Collection::default());
        heap.get_mut(id)
            .append(Value::Collection(id), None, 1)
            .unwrap();
        heap.collect(std::iter::once(id));
        assert_eq!(heap.get(id).items.len(), 1);
        heap.collect(std::iter::empty());
        assert!(heap.slots[id.0].is_none());
        assert_eq!(heap.allocate(Collection::default()), id);
    }

    #[test]
    fn collection_pass_preserves_active_loop_and_shadowed_value() {
        let mut output = Vec::new();
        run(
            r#"remember item as all of these: "saved"
remember scratch as all of these:
ok
for each item in (all of these: 4, 5):
    do this 200 times:
        set scratch to (all of these: item)
    ok
    say item
ok
say item
"#,
            &mut output,
        )
        .unwrap();
        assert_eq!(output, b"4\n5\n[\"saved\"]\n");
    }

    #[test]
    fn integer_decimal_comparison_uses_full_integer_precision() {
        let mut output = Vec::new();
        run(
            r#"remember 9007199254740993 as large
if large is greater than 9007199254740992.0:
    large but 1 more
    say large
ok
"#,
            &mut output,
        )
        .unwrap();
        assert_eq!(output, b"9007199254740994\n");
    }
}
