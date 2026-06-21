import { motion } from "framer-motion";
import { cn } from '../../../lib/utils';;

function Skeleton({
    className,
    ...props
}: React.HTMLAttributes<HTMLDivElement>) {
    return (
        <div
            className={cn(
                "relative overflow-hidden rounded-[5px] bg-[#272729]",
                className
            )}
            {...props}
        >
            <motion.div
                className="absolute inset-0 bg-gradient-to-r from-transparent via-[#1d1d1f]/50 to-transparent"
                initial={{ translateX: "-100%" }}
                animate={{ translateX: "100%" }}
                transition={{
                    repeat: Infinity,
                    duration: 1.5,
                    ease: "linear",
                }}
            />
        </div>
    );
}

export { Skeleton };
