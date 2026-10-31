import { useIntake } from "@/lib/intake";
import { IntakeCard } from "@/components/inspection-card";

/** The cards for everything dropped or pasted, newest first. */
export function IntakeList({ hideClear = false }: { hideClear?: boolean }) {
  const { items, notice, remove, clear } = useIntake();
  if (items.length === 0) return null;
  return (
    <div className="flex flex-col gap-3">
      {notice && <p className="text-[13px] text-status-warn">{notice}</p>}
      {items.length > 1 && !hideClear && (
        <button
          type="button"
          onClick={clear}
          className="self-end rounded-md text-xs text-muted-foreground outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
        >
          Clear all
        </button>
      )}
      {items.map((item) => (
        <IntakeCard key={item.key} item={item} onRemove={() => remove(item.key)} />
      ))}
    </div>
  );
}
