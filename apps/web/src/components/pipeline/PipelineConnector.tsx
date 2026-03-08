import { Component } from "solid-js";
import { clsx } from "clsx";

export interface PipelineConnectorProps {
  completed: boolean;
}

export const PipelineConnector: Component<PipelineConnectorProps> = (props) => {
  return (
    <div
      class={clsx(
        "w-8 h-0.5 rounded-full transition-colors duration-200",
        props.completed ? "bg-green" : "bg-border"
      )}
    />
  );
};
