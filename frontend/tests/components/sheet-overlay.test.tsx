import { afterAll, expect, mock, test } from 'bun:test';
import React from 'react';
import { create, type ReactTestRenderer } from 'react-test-renderer';

// The sheet is a thin wrapper over Radix Dialog, which needs a DOM. Only the
// `SheetContent` → `SheetOverlay` wiring is under test here, so the Dialog
// primitives are replaced with plain elements.
const originalDialog = { ...(await import('@radix-ui/react-dialog')) };
afterAll(() => {
  mock.module('@radix-ui/react-dialog', () => originalDialog);
});

mock.module('@radix-ui/react-dialog', () => ({
  Root: ({ children }: any) => <div>{children}</div>,
  Trigger: ({ children }: any) => <div>{children}</div>,
  Portal: ({ children }: any) => <div>{children}</div>,
  Overlay: ({ className, ...props }: any) => (
    <div data-testid="overlay" className={className} {...props} />
  ),
  Content: ({ children, className }: any) => (
    <section data-testid="content" className={className}>
      {children}
    </section>
  ),
  Close: ({ children }: any) => <button type="button">{children}</button>,
  Title: ({ children }: any) => <h2>{children}</h2>,
  Description: ({ children }: any) => <p>{children}</p>,
}));

const { Sheet, SheetContent } = await import('../../src/components/ui/sheet');

function overlayClass(renderer: ReactTestRenderer): string | undefined {
  return renderer.root.find((node) => node.props?.['data-testid'] === 'overlay').props
    .className;
}

test('the overlay keeps its default scrim when no class is given', async () => {
  let renderer!: ReactTestRenderer;
  await React.act(async () => {
    renderer = create(
      <Sheet open={true}>
        <SheetContent>Body</SheetContent>
      </Sheet>,
    );
  });

  const className = overlayClass(renderer) ?? '';
  expect(className).toContain('bg-black/80');
  await React.act(async () => renderer.unmount());
});

test('an overlay class is passed through to the overlay', async () => {
  let renderer!: ReactTestRenderer;
  await React.act(async () => {
    renderer = create(
      <Sheet open={true}>
        <SheetContent overlayClassName="bg-transparent">Body</SheetContent>
      </Sheet>,
    );
  });

  const className = overlayClass(renderer) ?? '';
  expect(className).toContain('bg-transparent');
  // The overlay stays mounted: an outside click still closes the sheet.
  expect(renderer.root.findAllByProps({ 'data-testid': 'overlay' })).toHaveLength(1);
  await React.act(async () => renderer.unmount());
});
