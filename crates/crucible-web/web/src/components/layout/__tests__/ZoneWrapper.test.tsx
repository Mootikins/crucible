import { describe, it, expect } from 'vitest';
import { render } from '@solidjs/testing-library';
import { ZoneWrapper } from '../ZoneWrapper';

describe('ZoneWrapper', () => {
  it('renders with data-zone attribute', () => {
    const { getByTestId } = render(() => (
      <ZoneWrapper zone="left" collapsed={false} />
    ));
    const el = getByTestId('zone-left');
    expect(el).toBeInTheDocument();
    expect(el.getAttribute('data-zone')).toBe('left');
  });

  it('applies sidebar flex styles when expanded', () => {
    const { getByTestId } = render(() => (
      <ZoneWrapper zone="left" collapsed={false} width={280} />
    ));
    const el = getByTestId('zone-left');
    expect(el.style.flexBasis).toBe('280px');
    expect(el.style.flexShrink).toBe('0');
    expect(el.style.flexGrow).toBe('0');
    expect(el.style.overflow).toBe('hidden');
  });

  it('collapses to zero flex-basis when collapsed', () => {
    const { getByTestId } = render(() => (
      <ZoneWrapper zone="left" collapsed={true} width={280} />
    ));
    const el = getByTestId('zone-left');
    expect(el.style.flexBasis).toBe('0px');
    expect(el.style.flexShrink).toBe('0');
    expect(el.style.flexGrow).toBe('0');
  });

  it('hides children when collapsed (non-center)', () => {
    const { getByTestId, queryByText } = render(() => (
      <ZoneWrapper zone="left" collapsed={true}>
        <span>child content</span>
      </ZoneWrapper>
    ));
    expect(getByTestId('zone-left')).toBeInTheDocument();
    expect(queryByText('child content')).not.toBeInTheDocument();
  });

  it('always shows children for center zone regardless of collapsed prop', () => {
    const { getByText } = render(() => (
      <ZoneWrapper zone="center" collapsed={false}>
        <span>center content</span>
      </ZoneWrapper>
    ));
    expect(getByText('center content')).toBeInTheDocument();
  });

  it('applies flex:1 and min-width:0 for center zone', () => {
    const { getByTestId } = render(() => (
      <ZoneWrapper zone="center" collapsed={false} />
    ));
    const el = getByTestId('zone-center');
    expect(el.style.flex).toBe('1 1 0%');
    expect(el.style.minWidth).toBe('0');
    expect(el.style.overflow).toBe('hidden');
  });

  it('uses height for bottom zone flex-basis', () => {
    const { getByTestId } = render(() => (
      <ZoneWrapper zone="bottom" collapsed={false} height={200} />
    ));
    const el = getByTestId('zone-bottom');
    expect(el.style.flexBasis).toBe('200px');
    expect(el.style.flexShrink).toBe('0');
    expect(el.style.flexGrow).toBe('0');
  });

  it('uses default width of 280px when no width prop given', () => {
    const { getByTestId } = render(() => (
      <ZoneWrapper zone="right" collapsed={false} />
    ));
    const el = getByTestId('zone-right');
    expect(el.style.flexBasis).toBe('280px');
  });

  it('uses default height of 200px when no height prop given for bottom', () => {
    const { getByTestId } = render(() => (
      <ZoneWrapper zone="bottom" collapsed={false} />
    ));
    const el = getByTestId('zone-bottom');
    expect(el.style.flexBasis).toBe('200px');
  });

  it('uses custom width when provided', () => {
    const { getByTestId } = render(() => (
      <ZoneWrapper zone="right" collapsed={false} width={350} />
    ));
    const el = getByTestId('zone-right');
    expect(el.style.flexBasis).toBe('350px');
  });
});
