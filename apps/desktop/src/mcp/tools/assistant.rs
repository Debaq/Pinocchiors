//! Lo que ayuda a la IA a trabajar con el usuario en la ventana: el plan del
//! proyecto (objetivo y pasos, que el usuario ve y edita en el panel de chat)
//! y las preguntas en forma de encuesta (el usuario contesta cada una).
//! Los dos viven en la ventana: sin ella (modo sin ventana) no están.

use serde_json::{json, Value};

use super::{arg_bool, arg_str, Ctx, Effect, Fail, Output, Tool};
use crate::mcp::schema::{array, boolean, integer, object, string};

pub const TOOLS: &[Tool] = &[PLAN, ASK_USER];

const NO_WINDOW: &str = "the plan and the questions live in the open Pinocchio window";
const NO_WINDOW_HINT: &str = "they work in live mode (the user turns on \"Let an AI drive this window\" in Settings ▸ AI connector) or from the app's own chat; without it, ask in plain text";

const PLAN: Tool = Tool {
    name: "plan",
    title: "Read or update the project plan",
    description: "The project's plan: a goal and numbered steps, saved inside the project and shown to the user at the \
top of the chat panel, where they can edit it too. Use it for any job of more than a couple of steps: set the `goal` \
and the `steps` when you start, mark steps `done` as you finish them, `add` steps you discover, and read it (no \
arguments) when you come back to a project — the user may have changed it. Steps are numbered from 1. \
`steps` replaces the whole list (all pending); `edit` rewrites steps ({\"3\": \"new text\"}); `insert` puts \
steps after a number ({\"after\": 2, \"steps\": [...]}, after 0 = at the start); `add` appends; `done` / `undone` / \
`remove` take step numbers; `clear: true` empties the plan. Keep it true to the job: when the user changes what they \
want, fix the steps right away. Several fields can go in one call; they apply in this order (numbers refer to the \
plan before the call): clear, goal, steps, edit, done, undone, remove, insert, add. Example: {\"goal\": \"Arduino box with a clear acrylic lid\", \"steps\": [\"Base 80×50×30 mm\", \
\"Lid 3 mm acrylic\", \"Holes for USB and screws\", \"2D drawing\"]}; later {\"done\": [1]}.",
    input: || {
        object(&[
            ("goal", string("What the user wants to achieve, in one or two lines"), false),
            ("steps", array("Replace the steps with these (all pending)", string("A step")), false),
            ("edit", json!({ "type": "object", "description": "Rewrite steps: {\"<number>\": \"new text\"}", "additionalProperties": { "type": "string" } }), false),
            ("insert", object(&[("after", integer("Insert after this step number (0 = at the start)", 0), true), ("steps", array("The new steps", string("A step")), true)]), false),
            ("add", array("Steps to append at the end", string("A step")), false),
            ("done", array("Step numbers (from 1) to mark done", integer("Step number", 1)), false),
            ("undone", array("Step numbers (from 1) to mark pending again", integer("Step number", 1)), false),
            ("remove", array("Step numbers (from 1) to delete", integer("Step number", 1)), false),
            ("clear", boolean("Empty the plan (goal and steps) before anything else"), false),
        ])
    },
    effect: Effect::Read,
    run: plan,
};

fn plan(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let Some(answer) = ctx.session.backend.ask_with("plan", args.clone()) else {
        return Err(Fail::new(NO_WINDOW).hint(NO_WINDOW_HINT));
    };
    let plan = answer?;
    let goal = plan["goal"].as_str().unwrap_or("").trim();
    let steps = plan["steps"].as_array().cloned().unwrap_or_default();
    let mut lines = vec![if goal.is_empty() { "Goal: (none yet)".to_string() } else { format!("Goal: {goal}") }];
    if steps.is_empty() {
        lines.push("No steps yet.".into());
    }
    for (i, s) in steps.iter().enumerate() {
        let mark = if s["done"] == true { "[x]" } else { "[ ]" };
        lines.push(format!("{} {mark} {}", i + 1, s["text"].as_str().unwrap_or("")));
    }
    let pending = steps.iter().filter(|s| s["done"] != true).count();
    if !steps.is_empty() {
        lines.push(if pending == 0 { "Every step is done.".into() } else { format!("{pending} step(s) pending.") });
    }
    Ok(Output::new(lines.join("\n"), plan))
}

const ASK_USER: Tool = Tool {
    name: "ask_user",
    title: "Ask the user (survey)",
    description: "Asks the user one or more questions as a survey in the chat panel: each question is shown on its own \
and the user must answer every one before sending (choosing options or writing). Use it whenever you need decisions \
or data you cannot find out yourself (sizes, materials, which option they prefer) instead of asking in plain text, \
and put all the questions you have in one call (up to 10). Give `options` when there are clear choices (the user can \
still write another answer unless `other: false`); `multiple: true` lets them pick several. Waits for the answers \
(the user may take a while) and returns them in order. Example: {\"title\": \"Lid\", \"questions\": [{\"question\": \
\"Lid material?\", \"options\": [\"Clear acrylic 3 mm\", \"Printed PLA\"]}, {\"question\": \"Inner height in mm?\"}]}.",
    input: || {
        object(&[
            ("title", string("Short title of the survey"), false),
            (
                "questions",
                json!({
                    "type": "array",
                    "description": "The questions, in order (1 to 10)",
                    "minItems": 1,
                    "maxItems": 10,
                    "items": object(&[
                        ("question", string("The question"), true),
                        ("options", array("Choices to pick from (none = free text)", string("A choice")), false),
                        ("multiple", boolean("Several choices can be picked"), false),
                        ("other", boolean("The user can write an answer of their own besides the choices (default true)"), false),
                    ]),
                }),
                true,
            ),
        ])
    },
    effect: Effect::Read,
    run: ask_user,
};

fn ask_user(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let questions = args["questions"].as_array().cloned().unwrap_or_default();
    if questions.is_empty() {
        return Err(Fail::new("no questions").hint("pass at least one {\"question\": ...}"));
    }
    let payload = json!({ "title": arg_str(args, "title"), "questions": questions });
    let Some(answer) = ctx.session.backend.ask_with("ask_user", payload) else {
        return Err(Fail::new(NO_WINDOW).hint(NO_WINDOW_HINT));
    };
    let a = answer.map_err(|e| Fail::new(e).hint("the user did not answer in time; go on with what you know or ask again later"))?;
    if arg_bool(&a, "skipped") == Some(true) {
        return Ok(Output::new("The user closed the survey without answering. Do not ask the same again right away: go on with sensible defaults and say which ones you chose, or ask in the chat.", json!({ "skipped": true })));
    }
    let answers = a["answers"].as_array().cloned().unwrap_or_default();
    let lines: Vec<String> = answers
        .iter()
        .enumerate()
        .map(|(i, x)| {
            let ans = match &x["answer"] {
                Value::Array(list) => list.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("; "),
                v => v.as_str().unwrap_or("").to_string(),
            };
            format!("{}. {} → {}", i + 1, x["question"].as_str().unwrap_or(""), ans)
        })
        .collect();
    Ok(Output::new(format!("The user answered:\n{}", lines.join("\n")), json!({ "answers": answers })))
}
