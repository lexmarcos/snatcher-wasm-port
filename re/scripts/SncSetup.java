// Pre-analysis script: create memory blocks, labels and functions from a labels file.
// args: <labels-file> [<memmap-file>]
// labels file lines:  ADDR NAME [f]      ('f' = also create a function, '#' comments)
// memmap lines:       ADDR SIZE NAME    (uninitialized volatile block, if not already mapped)
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.*;
import ghidra.program.model.mem.*;
import ghidra.program.model.symbol.*;
import java.nio.file.*;
import java.util.*;

public class SncSetup extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        if (args.length > 1) {
            for (String line : Files.readAllLines(Paths.get(args[1]))) {
                line = line.replaceAll("#.*", "").trim();
                if (line.isEmpty()) continue;
                String[] p = line.split("\\s+");
                Address a = toAddr(Long.decode(p[0]));
                long size = Long.decode(p[1]);
                Memory mem = currentProgram.getMemory();
                if (mem.getBlock(a) != null) continue;
                try {
                    MemoryBlock b = mem.createUninitializedBlock(p[2], a, size, false);
                    b.setVolatile(p[2].startsWith("io_"));
                    b.setWrite(true);
                } catch (Exception e) { println("block " + p[2] + ": " + e); }
            }
        }
        if (args.length > 0) applyLabels(args[0]);
        try { setAnalysisOption(currentProgram, "Aggressive Instruction Finder", "true"); } catch (Exception e) { println("opt: " + e); }
    }
    void applyLabels(String path) throws Exception {
        SymbolTable st = currentProgram.getSymbolTable();
        for (String line : Files.readAllLines(Paths.get(path))) {
            line = line.replaceAll("#.*", "").trim();
            if (line.isEmpty()) continue;
            String[] p = line.split("\\s+");
            Address a = toAddr(Long.decode(p[0]));
            try {
                st.createLabel(a, p[1], SourceType.USER_DEFINED).setPrimary();
                if (p.length > 2 && p[2].equals("f")) {
                    disassemble(a);
                    if (getFunctionAt(a) == null) createFunction(a, p[1]);
                }
            } catch (Exception e) { println("label " + p[1] + ": " + e); }
        }
    }
}
