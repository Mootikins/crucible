import { Component, JSX, ErrorBoundary as SolidErrorBoundary, createSignal } from "solid-js";
import { CLASSES } from "../flexlayout/core/Types";

export interface IErrorBoundaryProps {
    message: string;
    retryText: string;
    children: JSX.Element;
}

export const ErrorBoundary: Component<IErrorBoundaryProps> = (props) => {
    const [key, setKey] = createSignal(0);

    const retry = () => {
        setKey((k) => k + 1);
    };

    return (
        <SolidErrorBoundary
            fallback={(err) => {
                console.debug(err);
                return (
                    <div class={CLASSES.FLEXLAYOUT__ERROR_BOUNDARY_CONTAINER}>
                        <div class={CLASSES.FLEXLAYOUT__ERROR_BOUNDARY_CONTENT}>
                            <div style={{ display: "flex", "flex-direction": "column", "align-items": "center" }}>
                                {props.message}
                                <p><button onClick={retry}>{props.retryText}</button></p>
                            </div>
                        </div>
                    </div>
                );
            }}
        >
            {(() => {
                void key();
                return props.children;
            })()}
        </SolidErrorBoundary>
    );
};
