import { Component, For } from "solid-js";
import { clsx } from "clsx";
import { PIPELINE_STEPS, type PipelineStepId, type StepStatus } from "../../lib/pipeline";
import { PipelineNode } from "./PipelineNode";
import { PipelineConnector } from "./PipelineConnector";

export interface PipelineBarProps {
  getStepStatus: (stepId: PipelineStepId) => StepStatus;
  onStepClick: (stepId: PipelineStepId) => void;
}

export const PipelineBar: Component<PipelineBarProps> = (props) => {
  return (
    <div
      class={clsx(
        "flex items-center justify-center gap-3",
        "h-[4.5rem] px-8",
        "bg-bg-darker/50 border-b border-border"
      )}
    >
      <For each={PIPELINE_STEPS}>
        {(step, index) => (
          <>
            <PipelineNode
              icon={step.icon}
              label={step.label}
              status={props.getStepStatus(step.id)}
              onClick={() => props.onStepClick(step.id)}
            />
            {index() < PIPELINE_STEPS.length - 1 && (
              <PipelineConnector
                completed={props.getStepStatus(step.id) === "completed"}
              />
            )}
          </>
        )}
      </For>
    </div>
  );
};
