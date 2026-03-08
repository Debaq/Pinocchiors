import { Component } from "solid-js";
import { PlaceholderStep } from "./PlaceholderStep";
import * as Icons from "../icons";

export const AnimateStep: Component = () => (
  <PlaceholderStep
    title="Animar"
    description="Animación procedural y edición de poses: crear ciclos de caminata, idle y acciones personalizadas sobre el esqueleto."
    icon={Icons.Play}
  />
);
